#![feature(ptr_metadata)]
#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(pointer_is_aligned)]
#![feature(new_uninit)]
#![feature(split_array)]
#![feature(slice_ptr_len)]
#![feature(slice_ptr_get)]
#![feature(pointer_byte_offsets)]
#![feature(inline_const)]
#![feature(type_name_of_val)]
#![feature(arbitrary_self_types)]
#![feature(concat_bytes)]
#![feature(allocator_api)]
#![feature(iter_collect_into)]
#![no_main]
#![no_std]

extern crate alloc;

mod framebuffer;
mod config;
//mod ui;
//mod loadingscreen;
//mod elf;
mod paging;
mod logging;
mod elf;

use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::asm;
use core::fmt::Write;
use log::{debug, error, info, warn};
use uefi::fs::{Path, PathBuf};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::media::partition::PartitionInfo;
use uefi::table::boot::{AllocateType, MemoryDescriptor, MemoryType, OpenProtocolAttributes, OpenProtocolParams, PAGE_SIZE, SearchType};
use uefi::data_types::{Align, Identify};
use core::{fmt, mem, ptr};
use core::ops::{Deref, DerefMut};
use core::panic::PanicInfo;
use core::ptr::{NonNull, slice_from_raw_parts};
use bitflags::Flags;
use more_asserts::assert_lt;
use uefi::CString16;
use uefi::proto::console::serial::Serial;
use uefi::proto::loaded_image::LoadedImage;
use kernel_exports::ffi_abi;
use utils::handoff;
use utils::handoff::{ColorMask, MemoryMapEntry, Range};
use crate::config::Config;
use crate::framebuffer::{FontFamily, FontStyle, Tui};
use crate::paging::{Frame, MapError, Page, PageTable, TableEntryFlags};
use derive_more::Display;
use elf::header::program::{SegmentFlags, SegmentType};
use kernel_exports::memory::PhysicalAddress;

macro_rules! cstr {
    ($string:literal) => {
        unsafe { ::core::ffi::CStr::from_bytes_with_nul_unchecked(concat_bytes!($string, b'\0')) }
    };
}

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

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    let Ok(_) = uefi_services::init(&mut system_table) else {
        let _ = system_table.stderr().output_string(cstr16!("Unable to initialise")); // Can't really do anything if this fails
        return Status::ABORTED;
    };

    let services = system_table.boot_services();

    let Ok(uart) = services.get_handle_for_protocol::<Serial>() else {
        let _ = system_table.stderr().output_string(cstr16!("Unable to enable UART debugging")); // Can't really do anything if this fails
        return Status::PROTOCOL_ERROR;
    };

    let Ok(mut uart) = services.open_protocol_exclusive::<Serial>(uart) else {
        let _ = system_table.stderr().output_string(cstr16!("Unable to enable UART debugging")); // Can't really do anything if this fails
        return Status::PROTOCOL_ERROR;
    };

    let Ok(gop) = services.get_handle_for_protocol::<GraphicsOutput>() else {
        let _ = writeln!(uart, "Unable to enable graphics"); // Can't really do anything if this fails
        return Status::PROTOCOL_ERROR;
    };

    let Ok(mut gop) = (unsafe {
        services.open_protocol::<GraphicsOutput>(OpenProtocolParams {
            handle: gop,
            agent: services.image_handle(),
            controller: None,
        }, OpenProtocolAttributes::GetProtocol)
    }) else {
        let _ = writeln!(uart, "Unable to enable graphics"); // Can't really do anything if this fails
        return Status::PROTOCOL_ERROR;
    };

    // SAFETY: We don't touch the logger after calling exit_boot_services()
    // (unless someone breaks the code)
    unsafe { logging::init(&mut *uart).unwrap(); }

    let mut fs = services.get_image_file_system(image_handle).unwrap();

        /*let logo = fs.read(Path::new(cstr16!("EFI\\POPCORN\\logo.tga"))).unwrap();
    let logo = targa::Image::try_new(&logo).unwrap();

   let loading_screen = LoadingScreen::new(fb.dimensions(), ::ui::pixel::Color2{
        blue: 0x33,
        green: 0x33,
        red: 0x33,
        alpha: 0xff,
    }, logo);
    writeln!(loading_screen.text, "Hello world!").unwrap();

    let loading_screen = ui::Loading::new(logo, 0);
    fb.draw(loading_screen).unwrap();
    fb.flush();

    ::ui::rect::Rectangle::new();*/

    let Ok(config) = fs.read_to_string(Path::new(cstr16!(r"EFI\POPCORN\config.toml"))) else {
        panic!("Unable to find bootloader config file")
    };
    let config: Config = toml::from_str(&config).unwrap();

    let default_font = config.fonts.default;
    let default_font = config.fonts.font_list.get(default_font).unwrap_or_else(|| panic!("Unable to find default font `{}`", default_font));

    let regular = &default_font.regular;
    let bold = &default_font.bold;
    let italic = &default_font.italic;
    let bold_italic = &default_font.bold_italic;

    let regular = fs.read(regular).unwrap_or_else(|_| panic!("Unable to find regular font file"));
    let bold = bold.as_ref().map(|path| fs.read(path).unwrap_or_else(|_| panic!("Unable to find bold font file")));
    let italic = italic.as_ref().map(|path| fs.read(path).unwrap_or_else(|_| panic!("Unable to find italic font file")));
    let bold_italic = bold_italic.as_ref().map(|path| fs.read(path).unwrap_or_else(|_| panic!("Unable to bold-italic regular font file")));

    let regular = psf::try_parse(&regular).unwrap_or_else(|e| panic!("Invalid file for regular font: {}", e));
    let bold = bold.as_ref().map(|data| psf::try_parse(data).unwrap_or_else(|e| panic!("Invalid file for bold font: {}", e)));
    let italic = italic.as_ref().map(|data| psf::try_parse(data).unwrap_or_else(|e| panic!("Invalid file for italic font: {}", e)));
    let bold_italic = bold_italic.as_ref().map(|data| psf::try_parse(data).unwrap_or_else(|e| panic!("Invalid file for bold-italic font: {}", e)));

    let default_font = FontFamily::new(regular, bold, italic, bold_italic);
    let mut ui = Tui::new(&mut gop, (0,0), default_font);
    ui.set_font_style(FontStyle::Regular);
    ui.set_font_color(0xee, 0xee, 0xee);

    // SAFETY: We don't touch the logger after calling exit_boot_services()
    // (unless someone breaks the code)
    unsafe { logging::add_ui(&mut ui); }

    if let Ok(edid_handle) = services.get_handle_for_protocol::<framebuffer::ActiveEdid>()
	    && let Ok(edid) = services.open_protocol_exclusive::<framebuffer::ActiveEdid>(edid_handle) {
            info!("EDID: {:?}", &**edid);
        } else {
            warn!("Could not get EDID info");
        }


    // =========== test code using kernel from efi part ===========

    /*

    let modules = config.kernel_config.modules.into_iter().map(CString16::try_from)
                        .map(|r| r.map(PathBuf::from))
                        .collect::<Result<Vec<_>, _>>()
                        .expect("Invalid module path");

    let mut kernel = fs.read(Path::new(cstr16!(r"\EFI\POPCORN\kernel.exec"))).unwrap();
    let kernel = elf::File::try_new(&mut kernel).unwrap();

    // FIXME: This shouldn't just be KERNEL_CODE
    let kernel = elf::load_kernel(&mut kernel, |count, ty| services.allocate_pages(ty, memory_types::KERNEL_CODE, count))
            .expect("Unable to load kernel");
    let elf::KernelLoadInfo { kernel, mut page_table, address_range } = kernel;
    let mut address_range = {
        let aligned_start = address_range.start.0 & !(PAGE_SIZE - 1);
        let aligned_end = (address_range.end.0 + PAGE_SIZE - 1) / PAGE_SIZE * PAGE_SIZE;
        VirtualAddress(aligned_start)..VirtualAddress(aligned_end)
    };

    let kernel_symbols = kernel.exported_symbols();
    debug!("{:x?}", kernel_symbols);

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

        let mode_info = gop.current_mode_info();
        let mut framebuffer_info = gop.frame_buffer();
        let (width, height) = mode_info.resolution();

        let page_count = (framebuffer_info.size() + PAGE_SIZE - 1) / PAGE_SIZE;
        address_range.start -= page_count * PAGE_SIZE;
        let fb_start = address_range.start;

        let framebuffer_addr = framebuffer_info.as_mut_ptr() as usize;

        page_table.try_map_range_with::<(), _>(
            fb_start.try_into().unwrap(),
            Frame(framebuffer_addr.try_into().unwrap()),
            page_count.try_into().unwrap(),
            || services.allocate_pages(AllocateType::AnyPages, memory_types::PAGE_TABLE, 1).map_err(|_| ()),
            TableEntryFlags::WRITABLE | TableEntryFlags::NO_EXECUTE | TableEntryFlags::MMIO
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
            buffer: fb_start.0 as *mut u8,
            stride: mode_info.stride(),
            width,
            height,
            color_format
        }
    };

    let stack_top = {
        const STACK_PAGE_COUNT: usize = 32;
        address_range.start -= STACK_PAGE_COUNT*4096;

        let Ok(allocation) = services.allocate_pages(AllocateType::AnyPages, memory_types::KERNEL_STACK, STACK_PAGE_COUNT) else {
            panic!("Failed to allocate enough memory to load popcorn2");
        };

        page_table.try_map_range::<(), _>(address_range.start.try_into().unwrap(), Frame(allocation), STACK_PAGE_COUNT.try_into().unwrap(), || services.allocate_pages(AllocateType::AnyPages, memory_types::PAGE_TABLE, 1).map_err(|_| ()))
                         .unwrap();

        address_range.start + STACK_PAGE_COUNT*4096
    };

    let symbol_map = symbol_map.map(|m| NonNull::from(&mut *m.into_boxed_slice()));

    info!("new stack top at {:#x}", stack_top.0);

    // allocate before getting memory map from UEFI
    let mut kernel_mem_map = {
        let size = services.memory_map_size();
        Vec::with_capacity(size.map_size / size.entry_size + 16)
    };

    let mut memory_map_buffer = {
        let size = services.memory_map_size();
        let size = size.map_size + size.entry_size * 16;
        vec![0u8; size]
    };
    let mut memory_map = services.memory_map(
        MemoryDescriptor::align_buf(&mut memory_map_buffer).unwrap()
    ).unwrap();

    let stack_ptr: u64;
    unsafe { asm!("mov {}, rsp", out(reg) stack_ptr); }

    for mem in memory_map.entries().filter(|mem|
            mem.ty == MemoryType::LOADER_DATA ||
                    mem.ty == MemoryType::LOADER_CODE ||
                    (mem.phys_start..mem.phys_start + mem.page_count * 4096).contains(&stack_ptr)
    ) {
        debug!("{:x?} ({:#x} -> {:#x}) - {:?}", mem.ty, mem.phys_start, mem.phys_start + mem.page_count * 4096, mem.att);

        // UEFI memory sections are always aligned by firmware
        (0..mem.page_count).map(|page_num| mem.phys_start + page_num * 4096).try_for_each(|addr| {
            page_table.try_map_page::<(), _>(Page(addr), Frame(addr), || services.allocate_pages(AllocateType::AnyPages, memory_types::PAGE_TABLE, 1).map_err(|_| ()))
        }).unwrap();
    }

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
                memory_types::KERNEL_CODE => KernelCode,
                memory_types::PAGE_TABLE => KernelPageTable,
                memory_types::MODULE_CODE => ModuleCode,
                MemoryType::ACPI_NON_VOLATILE => AcpiPreserve,
                MemoryType::ACPI_RECLAIM => AcpiReclaim,
                memory_types::KERNEL_STACK => KernelStack,
                MemoryType::RUNTIME_SERVICES_CODE => RuntimeCode,
                MemoryType::RUNTIME_SERVICES_DATA => RuntimeData,
                _ => Reserved
            };

            MemoryMapEntry {
                coverage: Range(PhysicalAddress(descriptor.phys_start.try_into().unwrap()), PhysicalAddress((descriptor.phys_start + descriptor.page_count * 4096).try_into().unwrap())),
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
        kernel_mem_map
    };

    let kernel_entry = kernel.entrypoint();
    debug!("Handover to kernel with entrypoint at {:#x}", kernel_entry);

    drop(gop);
    drop(fs);
    drop(uart);

    let _ = system_table.exit_boot_services();
    kernel_page_table.switch();

    let handoff = handoff::Data {
        framebuffer: framebuffer_info,
        memory: handoff::Memory {
            map: kernel_mem_map,
            page_table_root: (&page_table).into(),
            stack: handoff::Stack {
                top: stack_top.0,
                bottom: address_range.start.0
            }
        },
        modules: handoff::Modules {
            phys_allocator_start: unsafe { mem::transmute(1usize) }
        },
        log: handoff::Logging {
            symbol_map
        },
        test: handoff::Testing {
            module_func: unsafe { mem::transmute(1usize) }
        }
    };

    let _ = system_table.exit_boot_services();
    page_table.switch();

    //type KernelStart = ffi_abi!(type fn(&handoff::Data) -> !);
    //let kernel_entry: KernelStart = unsafe { mem::transmute(kernel_entry) };
    unsafe {
        asm!("
            mov rsp, {}
            xor ebp, ebp
            call {}
        ", in(reg) stack_top.0, in(reg) kernel_entry, in("rdi") &handoff, options(noreturn))
    };

}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    error!("{}", info);
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

mod memory_types {
    use uefi::table::boot::MemoryType;

    pub const KERNEL_CODE: MemoryType = MemoryType::custom(0x8000_0000);
    pub const MODULE_CODE: MemoryType = MemoryType::custom(0x8000_0001);
    pub const PAGE_TABLE: MemoryType = MemoryType::custom(0x8000_0002);
    pub const MEMORY_ALLOCATOR_DATA: MemoryType = MemoryType::custom(0x8000_0003);
    pub const KERNEL_STACK: MemoryType = MemoryType::custom(0x8000_0004);
    pub const FRAMEBUFFER: MemoryType = MemoryType::custom(0x8000_0005);
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
