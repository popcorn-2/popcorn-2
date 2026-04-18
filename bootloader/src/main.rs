#![feature(try_blocks)]
#![feature(arbitrary_self_types)]
#![feature(step_trait)]
#![no_main]
#![no_std]

extern crate alloc;

use alloc::{format, vec};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::ffi::CString;
use alloc::vec::Vec;
use core::{fmt, mem};
use core::arch::asm;
use core::fmt::Write;
use core::iter::Step;
use core::ops::{Add, Deref};
use core::panic::PanicInfo;
use core::ptr::{NonNull, slice_from_raw_parts};
use core::time::Duration;

use bitflags::Flags;
use derive_more::Display;
use log::{debug, error, info, trace, warn};
use more_asserts::assert_lt;
use uefi::{Char16, CStr16, Event, Guid};
use uefi::data_types::{Align, Identify};
use uefi::fs::{FileSystem, Path};
use uefi::prelude::*;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput, PixelFormat};
use uefi::proto::console::pointer::Pointer;
use uefi::proto::console::serial::Serial;
use uefi::proto::console::text::{Input, Key};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::unsafe_protocol;
use uefi::table::boot::{AllocateType, MemoryDescriptor, MemoryType, OpenProtocolAttributes, OpenProtocolParams, PAGE_SIZE, SearchType};
use uefi::table::cfg;
use uefi_services::system_table;
use kernel_api::mapping::Ty;
use kernel_api::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};
use kernel_api::ptr::Unique;
use utils::handoff;
use utils::handoff::{ColorMask, MemoryMapEntry, Range};

use crate::config::Config;
use crate::paging::{Frame, MapError, Page, PageTable, TableEntryFlags};

mod config;
mod paging;
mod logging;
mod elf;

const PAGE_MAP_OFFSET: u64 = 0xffff_8000_0000_0000;
const PAGE_MAP_OFFSET_LEN: u64 = 2u64.pow(46);
const SHADOW_MAP_OFFSET: u64 = 0xffff_d000_0000_0000;

#[cfg(not(feature = "kasan"))] const STACK_PAGE_COUNT: usize = 34;
#[cfg(feature = "kasan")] const STACK_PAGE_COUNT: usize = 34*4;

struct DualWriter<T: Write, U: Write>(T, U);

impl<T: Write, U: Write> Write for DualWriter<T, U> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let a = self.0.write_str(s);
        let b = self.1.write_str(s);
        a?;
        b?;
        Ok(())
    }
}

#[repr(C)]
#[unsafe_protocol("bd8c1056-9f36-44ec-92a8-a6337f817986")]
pub struct ActiveEdid {
    edid_size: u32,
    edid_data: *const u8
}

impl Deref for ActiveEdid {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { &*slice_from_raw_parts(self.edid_data, self.edid_size.try_into().unwrap()) }
    }
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    let _ = writeln!(system_table.stdout(), "Loading popcorn2...");
    let Ok(_) = uefi_services::init(&mut system_table) else {
        let _ = writeln!(system_table.stdout(), "failed to init uefi services");
        return Status::ABORTED;
    };

    let services = system_table.boot_services();

    let uart = match services.get_handle_for_protocol::<Serial>() {
        Ok(uart) => uart,
        Err(e) => {
            error!("failed to init uart");
            return e.status();
        }
    };

    let mut uart = match unsafe {
        system_table.boot_services().open_protocol::<Serial>(OpenProtocolParams {
            handle: uart,
            agent: image_handle,
            controller: Some(image_handle),
        }, OpenProtocolAttributes::GetProtocol)
    } {
        Ok(uart) => uart,
        Err(e) => {
            error!("failed to init uart");
            return e.status()
        }
    };

    let gop = match services.get_handle_for_protocol::<GraphicsOutput>() {
        Ok(gop) => gop,
        Err(e) => {
            error!("failed to init graphics");
            return e.status();
        }
    };

    let mut gop = match unsafe {
        services.open_protocol::<GraphicsOutput>(OpenProtocolParams {
            handle: gop,
            agent: services.image_handle(),
            controller: None,
        }, OpenProtocolAttributes::GetProtocol)
    } {
        Ok(gop) => gop,
        Err(e) => {
            error!("failed to init graphics");
            return e.status();
        }
    };

    // SAFETY: We don't touch the logger after calling exit_boot_services()
    // (unless someone breaks the code)
    unsafe { logging::init(&mut *uart).unwrap(); }

    if let Ok(image) = services.open_protocol_exclusive::<LoadedImage>(image_handle) {
        info!("base address: {:p}", image.info().0);
    }

    let mouse = match services.get_handle_for_protocol::<Pointer>() {
        Ok(mouse) => mouse,
        Err(e) => {
            error!("Unable to find mouse");
            return e.status();
        }
    };

    let mut mouse = match services.open_protocol_exclusive::<Pointer>(mouse) {
        Ok(mouse) => mouse,
        Err(e) => {
            error!("Unable to find mouse");
            return e.status();
        }
    };

    let keyboard = match services.get_handle_for_protocol::<Input>() {
        Ok(keyboard) => keyboard,
        Err(e) => {
            error!("Unable to find keyboard");
            return e.status();
        }
    };

    let mut keyboard = match services.open_protocol_exclusive::<Input>(keyboard) {
        Ok(keyboard) => keyboard,
        Err(e) => {
            error!("Unable to find keyboard");
            return e.status();
        }
    };

    let size_mm = if let Ok(edid_handle) = services.get_handle_for_protocol::<ActiveEdid>()
            && let Ok(edid) = services.open_protocol_exclusive::<ActiveEdid>(edid_handle)
            && edid.len() > 71 {

        const DTD_OFFSET: usize = 54;
        let width_mm_lsb = edid[DTD_OFFSET + 12];
        let height_mm_lsb = edid[DTD_OFFSET + 13];
        let mm_msb = edid[DTD_OFFSET + 14];

        let width_mm = (width_mm_lsb as u16) | (((mm_msb as u16) & 0xF0) << 4);
        let height_mm = (height_mm_lsb as u16) | (((mm_msb as u16) & 0x0F) << 8);

        let size_mm = Some((width_mm, height_mm));

        info!("display size is {size_mm:?}");

        size_mm
    } else {
        warn!("Could not get EDID info");
        None
    };

    info!("Mouse: {:?}", mouse.mode());

	let mut verbose_mode = false;
	while let Ok(Some(key)) = keyboard.read_key() {
		debug!("{key:?}");

		const CHAR16_VL: Char16 = unsafe { Char16::from_u16_unchecked(b'v' as u16) };
		const CHAR16_VU: Char16 = unsafe { Char16::from_u16_unchecked(b'V' as u16) };

		match key {
			Key::Printable(CHAR16_VL | CHAR16_VU) => verbose_mode = true,
			_ => {}
		}

		if verbose_mode { break; } // Break once all options handled so it doesn't hang on holding keys
	}

	debug!("Verbose mode {verbose_mode}");

    let mut fs = match services.get_image_file_system(image_handle) {
        Ok(fs) => fs,
        Err(e) => return e.status()
    };

    /*
    let Ok(popfs_driver) = fs.read(Path::new(cstr16!(r"EFI\POPCORN\popfs.efi"))) else {
        panic!("Unable to find popfs driver")
    };
     TODO: Check if already loaded and if not, add to Driver#### efivars, adjust BootNext to point to uwave, then reboot
    let popfs_driver = services.load_image(image_handle, LoadImageSource::FromBuffer {

        buffer: &popfs_driver,
        file_path: None,
    }).unwrap();
    services.start_image(popfs_driver).unwrap();
     */

    let Ok(config) = fs.read_to_string(Path::new(cstr16!(r"EFI\POPCORN\config.toml"))) else {
        panic!("Unable to find bootloader config file")
    };
    let config: Config = toml::from_str(&config).unwrap();

    mouse.reset(false).unwrap();

    let (width, height) = gop.current_mode_info().resolution();
    let aspect = (width as f32) / (height as f32);
    let dpmm = size_mm.map(|(width_mm, height_mm)| {
        let dpmm_width = width / usize::from(width_mm);
        let dpmm_height = height / usize::from(height_mm);
        (dpmm_width + dpmm_height) / 2
    }).unwrap_or(50 /* 130 dpi */);

    let fb = (gop.frame_buffer().as_mut_ptr(), gop.frame_buffer().size(), gop.current_mode_info());
    
    const BOOTIMAGE_WIDTH: usize = 345;
    const BOOTIMAGE_HEIGHT: usize = 199;
    let bootimage = &include_bytes!("../../graphics/bootimage.bmp")[0x36..0x36+(4*BOOTIMAGE_WIDTH*BOOTIMAGE_HEIGHT)];

    let buffer: &[BltPixel] = unsafe {
        // SAFETY: alpha channel is 0 in BMP to comply with UEFI reserved byte requirements
        &*slice_from_raw_parts(
            bootimage.as_ptr().cast(),
            bootimage.len() / 4,
        )
    };

    let blt_op = BltOp::BufferToVideo {
        buffer,
        src: BltRegion::Full,
        dest: (10, 10),
        dims: (345, 199),
    };

    gop.blt(blt_op).expect("Failed to flush display");

    services.set_watchdog_timer(0, 0x10000, None).unwrap();
    
    if let Ok(image) = services.open_protocol_exclusive::<LoadedImage>(image_handle) {
        debug!("Loaded at base addr {:p}", image.info().0);
    }

    let (mut kernel, symbol_map, init_program, ramdisk) = locate_kernel(&image_handle, &services);


    // =========== test code using kernel from efi part ===========

    /*

    let modules = config.kernel_config.modules.into_iter().map(CString16::try_from)
                        .map(|r| r.map(PathBuf::from))
                        .collect::<Result<Vec<_>, _>>()
                        .expect("Invalid module path");

     */

    // FIXME: This shouldn't just be KERNEL_CODE
    let kernel = elf::load_kernel(&mut kernel, |count, ty| {
        let addr = services.allocate_pages(ty, MemoryType::LOADER_DATA, count)?;
        trace!("=== btl a {addr:#018x} -> {:#018x} : kernel executable", addr as usize + count * 4096);
        uefi::Result::Ok(addr)
    }).expect("Unable to load kernel");
    let elf::KernelLoadInfo { kernel, mut page_table, address_range, tls: kernel_tls } = kernel;
    let mut address_range = {
        address_range.start.align_down_to_page() .. address_range.end.align_up_to_page()
    };

    let kernel_symbols = kernel.exported_symbols();
    debug!("{:x?}", kernel_symbols);
    debug!("kernel tls data = {kernel_tls:x?}");
    debug!("kernel placed at {address_range:#x?}");

    /*let mut testing_fn: u64 = 0;
    for module in &modules {
        let result: Result<(),ModuleLoadError> = try {
            let base = kernel_last_page;
            info!("Loading module from `{}` at base address of {:#x}", module, base);
            let mut module = fs.read(module).map_err(|_| ModuleLoadError::FileNotFound)?;

            let module = {
                let mut module = ::elf::File::try_new(&mut module).map_err(|_| ModuleLoadError::InvalidElf)?;
                module.relocate(base.try_into().unwrap());
                module.link(&kernel_symbols).map_err(|e| ModuleLoadError::LinkingFailed(e.name().to_owned()))?;
                module
            };

            module.segments().filter(|segment| segment.segment_type == SegmentType::LOAD)
                  .try_for_each(|segment| {
                      let segment_vaddr = usize::try_from(segment.vaddr).unwrap();

                      let page_count = (usize::try_from(segment.memory_size).unwrap() + PAGE_SIZE - 1) / PAGE_SIZE;
                      let last_page = segment_vaddr + page_count * PAGE_SIZE;
                      if last_page > kernel_last_page { kernel_last_page = last_page; }

                      let Ok(allocation) = services.allocate_pages(AllocateType::AnyPages, memory_types::MODULE_CODE, page_count) else {
                          return Err(ModuleLoadError::Oom);
                      };

                      unsafe {
                          ptr::copy_nonoverlapping(module[segment.file_location()].as_ptr(), allocation as *mut _, segment.file_size.try_into().unwrap());
                          ptr::write_bytes((allocation + segment.file_size) as *mut u8, 0, (segment.memory_size - segment.file_size).try_into().unwrap());
                      }

                      kernel_page_table.try_map_range(Page(segment_vaddr.try_into().unwrap()), Frame(allocation.try_into().unwrap()), page_count.try_into().unwrap(), || todo!())
                                       .map_err(|e: MapError<()>| match e {
                                           MapError::AlreadyMapped => unreachable!(),
                                           MapError::SelfMapOverwrite => panic!("Attempted to overwrite page table self map"),
                                           MapError::AllocationError(_) => ModuleLoadError::Oom
                                       })?;

                      Ok(())
                  })?;

            let module_exports = module.exported_symbols();
            let mut author = "[UNKNOWN]";
            let mut fqn = "[UNKNOWN]";
            let mut name = Option::<&str>::None;

            if let Some(allocator_entrypoint) = module_exports.get(c"__popcorn_module_main_allocator") {
                testing_fn = allocator_entrypoint.value.get();
            }
            if let Some(symbol) = module_exports.get(c"__popcorn_module_author") {
                let author_data = module.data_at_address(symbol.value).unwrap();
                let author_data = unsafe { &*slice_from_raw_parts(author_data, symbol.size.try_into().unwrap()) };
                author = core::str::from_utf8(author_data).map_err(|_| ModuleLoadError::InvalidAuthorMetadata)?;
            }
            if let Some(symbol) = module_exports.get(c"__popcorn_module_modulename") {
                let name_data = module.data_at_address(symbol.value).unwrap();
                let name_data = unsafe { &*slice_from_raw_parts(name_data, symbol.size.try_into().unwrap()) };
                name = Some(core::str::from_utf8(name_data).map_err(|_| ModuleLoadError::InvalidNameMetadata)?);
            }
            if let Some(symbol) = module_exports.get(c"__popcorn_module_modulefqn") {
                let fqn_data = module.data_at_address(symbol.value).unwrap();
                let fqn_data = unsafe { &*slice_from_raw_parts(fqn_data, symbol.size.try_into().unwrap()) };
                fqn = core::str::from_utf8(fqn_data).map_err(|_| ModuleLoadError::InvalidFqnMetadata)?;
            }

            match name {
                Some(name) => info!("Loaded module `{name}` ({fqn}) by `{author}`"),
                None => info!("Loaded module `{fqn}` by `{author}`")
            }
        };

        if let Err(e) = result {
            panic!("Failed to load module: {e}")
        }
    }*/

    // map framebuffer
    let framebuffer_info: Option<handoff::Framebuffer> = try {
        use uefi::proto::console::gop::PixelBitmask;

        let mode_info = fb.2;
        let page_count = (fb.1 + PAGE_SIZE - 1) / PAGE_SIZE;
        address_range.start = address_range.start - page_count;
        let fb_start = address_range.start;

        let framebuffer_addr = fb.0 as usize;

        debug!("fb map {:#x} -> {:#x}", fb_start.addr, framebuffer_addr);

        page_table.try_map_range_with::<(), _>(
            Page(fb_start.addr.try_into().unwrap()),
            Frame(framebuffer_addr.try_into().unwrap()),
            page_count.try_into().unwrap(),
	        || {
                let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
                trace!("=== btl a {addr:#018x} -> {:#018x} : fb page tables", addr as usize + 4096);
                Ok(addr)
            },
	        TableEntryFlags::WRITABLE | TableEntryFlags::NO_EXECUTE | TableEntryFlags::MMIO,
	        Ty::FB,
        ).ok()?;

        let color_format = match mode_info.pixel_format() {
            PixelFormat::Rgb => Some(ColorMask::RGBX),
            PixelFormat::Bgr => Some(ColorMask::BGRX),
            PixelFormat::Bitmask => {
                let PixelBitmask{ red, green, blue, .. } = mode_info.pixel_bitmask().unwrap();
                Some(ColorMask{ red, green, blue })
            },
            PixelFormat::BltOnly => None
        }?;

        handoff::Framebuffer {
            buffer: unsafe { Unique::new(fb_start.addr as *mut u8) },
            stride: mode_info.stride(),
            width,
            height,
            color_format,
	        physical_address: PhysicalAddress::new(framebuffer_addr),
        }
    };

    let stack = {
        address_range.start = address_range.start - (STACK_PAGE_COUNT + 1); // `+ 1` for guard page

        let Ok(allocation) = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, STACK_PAGE_COUNT) else {
            panic!("Failed to allocate enough memory to load popcorn2");
        };
        trace!("=== btl a {allocation:#018x} -> {:#018x} : kernel stack", allocation as usize + STACK_PAGE_COUNT * 4096);

        debug!("stack map {:#x} -> {:#x}", address_range.start.addr+4096, allocation);

        page_table.try_map_range_with::<(), _>(Page((address_range.start.addr+4096).try_into().unwrap()), Frame(allocation), STACK_PAGE_COUNT.try_into().unwrap(), || {
            let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
            trace!("=== btl a {addr:#018x} -> {:#018x} : stack page table", addr as usize + 4096);
            Ok(addr)
        }, TableEntryFlags::NO_EXECUTE | TableEntryFlags::WRITABLE, Ty::KERNEL_STACK)
                         .unwrap();

        handoff::Stack {
            bottom_virt: address_range.start,
            top_virt: address_range.start + STACK_PAGE_COUNT + 1usize,
            top_phys: RawFrame::new(allocation.try_into().unwrap()) + STACK_PAGE_COUNT,
        }
    };

	#[cfg(feature = "kasan")]
	let mut allocate_shadow_memory = |page_table: &mut PageTable, start: RawPage, end: RawPage, val: u8| {
		use kernel_api::memory::asan;

		// fixme: this incorrectly unpoisons memory below and above the actual location
		let shadow_start = {
			let val = (start.addr >> 3) + asan::SHADOW_MAP_SHIFT;
			let aligned = VirtualAddress::new(val).align_down_to_page();
			Page(aligned.addr as u64)
		};
		let shadow_end = {
			let val = (end.addr >> 3) + asan::SHADOW_MAP_SHIFT;
			let aligned = VirtualAddress::new(val).align_up_to_page();
			Page(aligned.addr as u64)
		};
		debug!("map shadow memory for {shadow_start:x?} to {shadow_end:x?}");

		let page_count = (shadow_end.0 - shadow_start.0) / 4096;

		let Ok(allocation) = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, page_count as usize) else {
			panic!("Failed to allocate enough memory for shadow memory");
		};
		trace!("=== btl a {allocation:#018x} -> {:#018x} : shadow memory", allocation + 4096*page_count);

		let mut allocate = || -> Result<u64, ()> {
			let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
			trace!("=== btl a {addr:#018x} -> {:#018x} : shadow memory page table", addr as usize + 4096);
			Ok(addr)
		};

		for i in 0..page_count {
			let page = Page(shadow_start.0 + i*4096);
			let frame = Frame(Frame(allocation).0 + i*4096);

			match page_table.try_map_page_with(page, frame, &mut allocate, TableEntryFlags::NO_EXECUTE | TableEntryFlags::WRITABLE, Ty::SHADOW_MEM) {
				Ok(_) => {
					unsafe {
						core::ptr::write_bytes(
							frame.0 as *mut u8,
							val,
							4096,
						);
					}
				},
				Err(MapError::AlreadyMapped(Ty::SHADOW_MEM)) => {
					let translated = page_table.translate_page(page).expect("just got an error trying to map over this");
					let read = unsafe { (translated.0 as *mut u8).read() };
					if read != val { todo!("{read:#x} -> {val:#x} ({i})") }
					//services.free_pages(frame.0, 1).unwrap();
					//trace!("=== btl d {:#018x} -> {:#018x}", frame.0, frame.0 + 4096);
					warn!("oopsie doopsie we're leaking memory ({:#018x})", frame.0);
				},
				e => e.unwrap(),
			}
		}

		unsafe {
			core::ptr::write_bytes(
				allocation as *mut u8,
				val,
				(shadow_end.0 - shadow_start.0) as usize
			);
		}

		(shadow_start, allocation)
	};

    #[cfg(feature = "kasan")] {
	    use kernel_api::memory::asan;

        // set up shadow memory for stack, framebuffer, and kernel executable
        let core::ops::Range { start, end } = address_range.clone();

	    let (true_start, allocation) = allocate_shadow_memory(&mut page_table, start, end, 0x00);

	    let stack_shadow_offset = ((stack.bottom_virt.addr >> 3) + asan::SHADOW_MAP_SHIFT) - (true_start.0 as usize);
	    debug!("set {} bytes of shadow to 0xf4 for stack guard page", 4096 / 8);
	    debug!("set {} bytes of shadow to 0 for stack memory", STACK_PAGE_COUNT * 4096 / 8);
	    unsafe {
		    let stack_guard_shadow = (allocation as *mut u8).byte_add(stack_shadow_offset);
		    let stack_shadow = stack_guard_shadow.byte_add(4096 / 8);
		    debug!("stack_shadow = {stack_guard_shadow:#p}");
		    core::ptr::write_bytes(
			    stack_guard_shadow,
			    0xf4,
			    4096 / 8
		    );
		    core::ptr::write_bytes(
			    stack_shadow,
			    0,
			    STACK_PAGE_COUNT * 4096 / 8
		    );
	    }

        // set up shadow mem for shadow memory
	    /*allocate_shadow_memory(
		    &mut page_table,
		    asan::SHADOW_MAP_START.align_down_to_page(),
		    asan::SHADOW_MAP_END.align_up_to_page(),
		    0xcc,
	    );*/
    }

    let symbol_map = symbol_map.map(|m| &*Box::leak(m.into_boxed_slice()));
    let init_program = &*Box::leak(init_program.into_boxed_slice());
    let ramdisk = &*Box::leak(ramdisk.into_boxed_slice());

    info!("new stack at {:#x?}", stack);

    let mut memory_map_buffer = {
        let size = services.memory_map_size();
        let size = size.map_size + size.entry_size * 16;
        vec![0u8; size]
    };

    // Allocate handoff upfront so it's in the memory map and gets mapped into kernel page tables
    let handoff = Box::leak(Box::<handoff::Data>::new_uninit());
    debug!("Allocated handoff structure at {handoff:#p} -> {:#p}", handoff.as_ptr().wrapping_offset(1));
    debug!(
        "Allocated memmap structure at {:#p} -> {:#p}",
        memory_map_buffer.as_ptr(),
        memory_map_buffer.as_ptr().wrapping_byte_offset(memory_map_buffer.len() as isize)
    );

	// Allocate mem map buffer upfront so it's in the memory map and gets mapped into kernel page tables
	let mut kernel_mem_map = {
		let size = services.memory_map_size();
		Vec::with_capacity(size.map_size / size.entry_size + 64) // add more capacity because we have a bunch of allocations after this
	};
    
    let mut memory_map = services.memory_map(
        MemoryDescriptor::align_buf(&mut memory_map_buffer).unwrap()
    ).unwrap();

    let stack_ptr: u64;
    unsafe { asm!("mov {}, rsp", out(reg) stack_ptr); }

	// create page map region
    for mem in memory_map.entries().filter(|mem|
            mem.ty == MemoryType::BOOT_SERVICES_CODE ||
            mem.ty == MemoryType::BOOT_SERVICES_DATA ||
            mem.ty == MemoryType::PERSISTENT_MEMORY ||
            mem.ty == MemoryType::LOADER_CODE ||
            mem.ty == MemoryType::LOADER_DATA ||
            mem.ty == MemoryType::ACPI_NON_VOLATILE ||
            mem.ty == MemoryType::ACPI_RECLAIM ||
            mem.ty == MemoryType::RUNTIME_SERVICES_CODE ||
            mem.ty == MemoryType::RUNTIME_SERVICES_DATA ||
            mem.ty == MemoryType::CONVENTIONAL
    ) {
        debug!("[page map] {:x?} ({:#x} -> {:#x}) -> {:#x}", mem.ty, mem.phys_start, mem.phys_start + mem.page_count * 4096, mem.phys_start + PAGE_MAP_OFFSET);

        // UEFI memory sections are always aligned by firmware
        (0..mem.page_count).map(|page_num| mem.phys_start + page_num * 4096).try_for_each(|addr| {
            let virt_addr = addr + PAGE_MAP_OFFSET;
            assert!(addr < PAGE_MAP_OFFSET_LEN, "Too much physical memory");
            page_table.try_map_page_with::<(), _>(Page(virt_addr), Frame(addr), || {
                let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
                trace!("=== btl a {addr:#018x} -> {:#018x} : page map page table", addr as usize + 4096);
                Ok(addr)
            }, TableEntryFlags::NO_EXECUTE | TableEntryFlags::WRITABLE, Ty::MEM_MAP)
        }).unwrap();

	    #[cfg(feature = "kasan")]
	    {
		    let virt_start = VirtualAddress::new((mem.phys_start + PAGE_MAP_OFFSET) as usize).align_up_to_page();
		    let virt_end = VirtualAddress::new((mem.phys_start + PAGE_MAP_OFFSET) as usize)
				    .add(mem.page_count as usize * 4096)
				    .align_down_to_page();

		    allocate_shadow_memory(
			    &mut page_table,
			    virt_start,
			    virt_end,
			    0x00,
		    );
	    }
    }

	// identity map bootloader data and bootloader stack
    for mem in memory_map.entries().filter(|mem|
            mem.ty == MemoryType::LOADER_DATA ||
                    (mem.phys_start..mem.phys_start + mem.page_count * 4096).contains(&stack_ptr)
    ) {
        debug!("{:x?} ({:#x} -> {:#x}) - {:?}", mem.ty, mem.phys_start, mem.phys_start + mem.page_count * 4096, mem.att);

        // UEFI memory sections are always aligned by firmware
        (0..mem.page_count).map(|page_num| mem.phys_start + page_num * 4096).try_for_each(|addr| {
            page_table.try_map_page_with::<(), _>(Page(addr), Frame(addr), || {
                let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
                trace!("=== btl a {addr:#018x} -> {:#018x} : bootloader data page table", addr as usize + 4096);
                Ok(addr)
            }, TableEntryFlags::NO_EXECUTE | TableEntryFlags::WRITABLE, Ty::LOADER_DATA)
        }).unwrap();
    }

	// identity map bootloader code
    for mem in memory_map.entries().filter(|mem| mem.ty == MemoryType::LOADER_CODE) {
        debug!("{:x?} ({:#x} -> {:#x}) - {:?}", mem.ty, mem.phys_start, mem.phys_start + mem.page_count * 4096, mem.att);

        // UEFI memory sections are always aligned by firmware
        (0..mem.page_count).map(|page_num| mem.phys_start + page_num * 4096).try_for_each(|addr| {
            page_table.try_map_page_with::<(), _>(Page(addr), Frame(addr), || {
                let addr = services.allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).map_err(|_| ())?;
                trace!("=== btl a {addr:#018x} -> {:#018x} : bootloader code page table", addr as usize + 4096);
                Ok(addr)
            }, TableEntryFlags::WRITABLE, Ty::LOADER_CODE)
        }).unwrap();
    }

	// since the allocations above will have affected the memory map, regenerate it before converting to kernel handoff format
	{
		let size = services.memory_map_size();
		let size = size.map_size + size.entry_size * 16;
		memory_map_buffer.reserve(
			size.saturating_sub(memory_map_buffer.capacity())
		);
	}

	let mut memory_map = services.memory_map(
		MemoryDescriptor::align_buf(&mut memory_map_buffer).unwrap()
	).unwrap();

    debug!("Generating kernel memory map");
    let kernel_mem_map = {
        let descriptor_to_entry = |descriptor: &MemoryDescriptor| {
            use handoff::MemoryType::*;

            let mut ty = match descriptor.ty {
                MemoryType::CONVENTIONAL |
                MemoryType::BOOT_SERVICES_CODE |
                MemoryType::BOOT_SERVICES_DATA |
                MemoryType::PERSISTENT_MEMORY => Free,
                MemoryType::LOADER_CODE => BootloaderCode,
                MemoryType::LOADER_DATA => BootloaderData,
                MemoryType::ACPI_NON_VOLATILE => AcpiPreserve,
                MemoryType::ACPI_RECLAIM => AcpiReclaim,
                MemoryType::RUNTIME_SERVICES_CODE => RuntimeCode,
                MemoryType::RUNTIME_SERVICES_DATA => RuntimeData,
                _ => Reserved
            };

            MemoryMapEntry {
                coverage: Range(PhysicalAddress::new(descriptor.phys_start.try_into().unwrap()), PhysicalAddress::new((descriptor.phys_start + descriptor.page_count * 4096).try_into().unwrap())),
                ty
            }
        };

        memory_map.sort();
        let mem_map_data = memory_map.entries().map(|entry| {
            descriptor_to_entry(entry)
        });
        assert_lt!(mem_map_data.len(), kernel_mem_map.capacity());
        let last_item = mem_map_data.reduce(|old_item, new_item| {
            if old_item.ty == new_item.ty && old_item.coverage.1 == new_item.coverage.0 {
                MemoryMapEntry {
                    ty: old_item.ty,
                    coverage: Range(old_item.coverage.0, new_item.coverage.1)
                }
            } else {
                kernel_mem_map.push(old_item);
                new_item
            }
        });
        if let Some(item) = last_item {
            kernel_mem_map.push(item);
        };
        Vec::leak(kernel_mem_map)
    };

    let kernel_entry = kernel.entrypoint();
    debug!("Handover to kernel with entrypoint at {:#x}", kernel_entry);

    drop(gop);
    drop(fs);
    drop(uart);
    drop(mouse);
    drop(keyboard);

    let config_tables = system_table.config_table();
    let rsdp = if let Some(xsdp) = config_tables.iter().find(|table| table.guid == cfg::ACPI2_GUID) {
        PhysicalAddress::new(xsdp.address as usize)
    } else if let Some(rsdp) = config_tables.iter().find(|table| table.guid == cfg::ACPI_GUID) {
        PhysicalAddress::new(rsdp.address as usize)
    } else {
        panic!("No RSDP found");
    };

    let handoff = handoff.write(handoff::Data {
        framebuffer: framebuffer_info,
        memory: handoff::Memory {
            map: kernel_mem_map,
            used: Range(address_range.start, address_range.end),
            stack,
        },
        modules: handoff::Modules {

        },
        log: handoff::Logging {
            symbol_map: symbol_map.map(NonNull::from),
        },
        test: handoff::Testing {
            module_func: unsafe { mem::transmute(1usize) }
        },
        tls: (Range(kernel_tls.0.start, kernel_tls.0.end), kernel_tls.1),
        rsdp,
        init_exec: init_program,
        ramdisk,
    });

	trace!("{handoff:x?}");

    let _ = system_table.exit_boot_services();

    unsafe {
        // Enable write-protect bit
        asm!(
            "mov {0:r}, cr0",
            "or {0:r}, 0x10000",
            "mov cr0, {0:r}",
            out(reg) _
        );
        // Enable NX enable and syscall/sysret bits
        asm!(
            "rdmsr",
            "or eax, 0x801",
            "wrmsr",
            in("ecx") 0xC0000080u32,
            out("eax") _,
            out("edx") _
        );
    }

    page_table.switch();

    //type KernelStart = ffi_abi!(type fn(&handoff::Data) -> !);
    //let kernel_entry: KernelStart = unsafe { mem::transmute(kernel_entry) };
    unsafe {
        asm!(
            "mov rsp, rcx",
            "xor ebp, ebp",
            "push 0",

            "mov eax, 0xead10ca1",
            "mov edx, 0xd", // edx:eax = 0xdead10ca1 ('dead local')
            "mov ecx, 0xc0000101", // ecx = GSBase MSR
            "wrmsr",

            "jmp rsi",
        in("rcx") stack.top_virt.addr, in("rsi") kernel_entry, in("rdi") handoff, options(noreturn))
    }
}

fn locate_kernel(image_handle: &Handle, services: &BootServices) -> (Vec<u8>, Option<Vec<u8>>, Vec<u8>, Vec<u8>) {
    // FIXME: this doesn't check which disk is being used so it'll happily load popcorn from any random disk

    let mut root_partition_handle: Option<Handle> = None;
    if let Ok(partition_handles) = services.locate_handle_buffer(SearchType::ByProtocol(&const { Guid::parse_or_panic("8A6CC16C-D110-46F1-813F-0382046342C8") })) {
        for partition_handle in partition_handles.iter() {
            // todo: which one to load if multiple
            root_partition_handle = Some(*partition_handle);
            break;
        }
    }

    let root_partition_handle = root_partition_handle.expect("No popcorn system disk found");

    let mut fs = {
        debug!("root partition protos: {:?}", services.protocols_per_handle(root_partition_handle).as_deref());

        let Ok(proto) = services.open_protocol_exclusive::<SimpleFileSystem>(root_partition_handle) else {
            panic!()
        };

        FileSystem::new(proto)
    };

    // TODO: versioning
    let symbol_map = fs.read(Path::new(cstr16!(r"\kernel\kernel.map"))).ok();
    let init_program = fs.read(Path::new(cstr16!(r"\user\init.exec"))).expect("Could not find `init`");
    let ramdisk = fs.read(Path::new(cstr16!(r"\user\init.tar"))).expect("Could not find ramdisk");
    let kernel_data = fs.read(Path::new(cstr16!(r"\kernel\kernel.exec"))).expect("Unable to find a bootable kernel");

    /*
    let symbol_map = fs.read(Path::new(cstr16!(r"\EFI\POPCORN\symbols.map")))
                       .ok().map(|v| {
        debug!("{:x?}", &v[0..10]);
        let p = Box::into_raw(v.into_boxed_slice());
        unsafe { NonNull::new_unchecked(p) }
    });
     */
    (kernel_data, symbol_map, init_program, ramdisk)
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    error!("{}", info);

    #[derive(Debug)]
    struct Counter { count: usize }
    impl Write for Counter {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            self.count += s.len();
            Ok(())
        }
    }

    let mut c = Counter { count: 1 };
    write!(c, "{info}");
    debug!("{c:?}");

    let tab = unsafe { system_table().as_mut() };

    write!(tab.stderr(), "{}", info);

    if let Ok(raw_buffer) = tab.boot_services().allocate_pool(MemoryType::BOOT_SERVICES_DATA, c.count * 2) {
        struct Writer {
            start: *mut u16,
            idx: usize,
            max: usize,
        }
        impl Write for Writer {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                for c in s.chars() {
                    if c.is_ascii() {
                        if self.idx >= self.max { return Err(fmt::Error); }
                        unsafe { self.start.add(self.idx).write(c as u16); }
	                    self.idx += 1;
                    }
                }

                Ok(())
            }
        }

        let mut w = Writer {
            start: raw_buffer.cast(),
            idx: 0,
            max: c.count - 1
        };
        write!(&mut w, "{info}");
        unsafe { w.start.add(w.idx).write(0); }

        let buffer = unsafe { &*slice_from_raw_parts(w.start, w.idx + 1) };
        debug!("exit buffer: {buffer:?}");
        if let Ok(buffer) = CStr16::from_u16_with_nul(buffer) {
            debug!("exit buffer: {buffer:?}");
            tab.boot_services().stall(10_000_000);
            unsafe { tab.boot_services().exit(tab.boot_services().image_handle(), Status::ABORTED, c.count, buffer.as_ptr().cast_mut()); }
        }
    } else {
        warn!("exit message buffer not allocated");
    }

    loop {}
}

/*
| UEFI type                  | Use                                                              |
|----------------------------|------------------------------------------------------------------|
| EfiReservedMemoryType      | Unusable memory                                                  |
| EfiLoaderCode              | Bootloader code                                                  |
| EfiLoaderData              | Bootloader data and memory allocations by bootloader             |
| EfiBootServicesCode        | Boot services driver code - preserve to use boot services        |
| EfiBootServicesData        | Boot services driver data - preserve to use boot services        |
| EfiRuntimeServicesCode     | Runtime services driver code - preserve to use runtime services |
| EfiRuntimeServicesData     | Runtime services driver data - preserve to use runtime services |
| EfiConventionalMemory      | Free memory                                                      |
| EfiUnusableMemory          | Memory with errors detected                                      |
| EfiACPIReclaimMemory       | Memory containing ACPI tables - preserve until parsing ACPI      |
| EfiACPIMemoryNVS           | ACPI firmware memory that must be preserved across sleep         |
| EfiMemoryMappedIO          | ???                                                              |
| EfiMemoryMappedIOPortSpace | ???                                                              |
| EfiPalCode                 | ???                                                              |
| EfiPersistentMemory        | Non-volatile but otherwise conventional memory                   |
| EfiUnacceptedMemoryType    | ???                                                              |
 */

mod paging_reasons {
	use elf::header::program::{SegmentFlags, SegmentType};
	use kernel_api::mapping::Ty;

	pub fn kernel_seg_to_mapping_ty(ty: SegmentType, flags: SegmentFlags) -> Ty {
		match ty {
			SegmentType::LOAD if flags.contains(SegmentFlags::Executable) => Ty::KERNEL_CODE,
			SegmentType::LOAD => Ty::KERNEL_DATA,
			SegmentType::TLS => Ty::KERNEL_TLS,
			_ => Ty::KERNEL_OTHER,
		}
	}
}

#[derive(Display)]
enum ModuleLoadError {
    #[display(fmt = "Could not locate requested module")]
    FileNotFound,
    #[display(fmt = "Module file is corrupted")]
    InvalidElf,
    #[display(fmt = "Failed to resolve symbol {_0:?}")]
    LinkingFailed(CString),
    #[display(fmt = "Could not allocate memory for module")]
    Oom,
    #[display(fmt = "Invalid data in `author` metadata")]
    InvalidAuthorMetadata,
    #[display(fmt = "Invalid data in `name` metadata")]
    InvalidNameMetadata,
    #[display(fmt = "Invalid data in `fqn` metadata")]
    InvalidFqnMetadata,
}
