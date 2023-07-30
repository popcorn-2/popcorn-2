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
#![feature(adt_const_params)]
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

use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::asm;
use core::fmt::Write;
use bitflags::Flags;
use log::{debug, error, info, warn};
use uefi::fs::{Path, PathBuf};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::proto::media::partition::PartitionInfo;
use uefi::table::boot::{AllocateType, MemoryDescriptor, MemoryType, OpenProtocolAttributes, OpenProtocolParams, PAGE_SIZE, ScopedProtocol, SearchType};
use uefi::data_types::{Align, Identify};
use core::{fmt, mem, ptr};
use core::ffi::CStr;
use core::mem::{align_of, discriminant, size_of};
use core::ops::{Deref, DerefMut};
use core::panic::PanicInfo;
use core::ptr::slice_from_raw_parts;
use hashbrown::HashMap;
use more_asserts::assert_lt;
use uefi::{CString16, Error};
use uefi::proto::console::serial::Serial;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::block::BlockIO;
use kernel_exports::ffi_abi;
use utils::handoff;
use utils::handoff::{ColorMask, MemoryMapEntry, Range};
use utils::handoff::MemoryType::Reserved;
use crate::config::Config;
use crate::framebuffer::{FontFamily, FontStyle, Tui};
use crate::paging::{Frame, MapError, Page, PageTable, TableEntryFlags};
use derive_more::Display;
use elf::ExecutableAddressRelocated;
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
    unsafe { logging::init(uart.deref_mut()).unwrap(); }

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
            info!("EDID: {:?}", edid.deref().deref());
        } else {
            warn!("Could not get EDID info");
        }


        // =========== test code using kernel from efi part ===========

    let modules = config.kernel_config.modules.into_iter().map(CString16::try_from)
            .map(|r| r.map(PathBuf::from))
            .collect::<Result<Vec<_>, _>>()
            .expect("Invalid module path");

    let mut kernel = fs.read(Path::new(cstr16!(r"\EFI\POPCORN\kernel.exec"))).unwrap();
    let kernel = elf::File::try_new(&mut kernel).unwrap();

    let page_table_allocator_fn = || services.allocate_pages(AllocateType::AnyPages, memory_types::PAGE_TABLE, 1);

    let mut kernel_page_table = unsafe { PageTable::try_new(page_table_allocator_fn) }.unwrap();
    let mut kernel_last_page = usize::MIN;
    let mut kernel_first_page = usize::MAX;

    kernel.segments().filter(|segment| segment.segment_type == SegmentType::LOAD)
          .for_each(|segment| {
              let allocation_type = if segment.segment_flags.contains(SegmentFlags::LowMem) {
                  AllocateType::MaxAddress(0x10_0000)
              } else { AllocateType::AnyPages };
              let page_count = (usize::try_from(segment.memory_size).unwrap() + PAGE_SIZE - 1) / PAGE_SIZE;
              let last_page = usize::try_from(segment.vaddr).unwrap() + page_count * PAGE_SIZE;
              if last_page > kernel_last_page { kernel_last_page = last_page; }
              if usize::try_from(segment.vaddr).unwrap() < kernel_first_page { kernel_first_page = usize::try_from(segment.vaddr).unwrap(); }

              let Ok(allocation) = services.allocate_pages(allocation_type, memory_types::KERNEL_CODE, page_count) else {
                  panic!("Failed to allocate enough memory to load popcorn2");
              };

              unsafe {
                  ptr::copy_nonoverlapping(kernel[segment.file_location()].as_ptr(), allocation as *mut _, segment.file_size.try_into().unwrap());
                  ptr::write_bytes((allocation + segment.file_size) as *mut u8, 0, (segment.memory_size - segment.file_size).try_into().unwrap());
              }

              (0..page_count).map(|page| ((page * PAGE_SIZE) + usize::try_from(segment.vaddr).unwrap(), (page * PAGE_SIZE) + usize::try_from(allocation).unwrap()))
                             .try_for_each(|(virtual_addr, physical_addr)| {
                                 kernel_page_table.try_map_page(Page(virtual_addr.try_into().unwrap()), Frame(physical_addr.try_into().unwrap()), page_table_allocator_fn)
                             })
                             .unwrap();
          });

	let kernel_symbols = kernel.exported_symbols();
    debug!("{:x?}", kernel_symbols);

    let mut testing_fn: u64 = 0;
    for module in &modules {
        let result: Result<(),ModuleLoadError> = try {
            let base = kernel_last_page;
            info!("Loading module from `{}` at base address of {:#x}", module, base);
            let mut module = fs.read(module).map_err(|_| ModuleLoadError::FileNotFound)?;

            let module = {
                let mut module = elf::File::try_new(&mut module).map_err(|_| ModuleLoadError::InvalidElf)?;
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

                    (0..page_count).map(|page| ((page * PAGE_SIZE) + segment_vaddr, (page * PAGE_SIZE) + usize::try_from(allocation).unwrap()))
                        .try_for_each(|(virtual_addr, physical_addr)| {
                            kernel_page_table.try_map_page(Page(virtual_addr.try_into().unwrap()), Frame(physical_addr.try_into().unwrap()), page_table_allocator_fn)
                        })
                        .map_err(|e| match e {
                            MapError::AlreadyMapped => unreachable!(),
                            MapError::AllocationError(_) => ModuleLoadError::Oom
                        })?;

                    Ok(())
                })?;

            let symtab = module.dynamic_symbol_table().unwrap();
            let stringtab = module.dynamic_string_table().unwrap();

            let module_exports = module.exported_symbols();
            let mut author = "[UNKNOWN]";
            let mut fqn = "[UNKNOWN]";
            let mut name = Option::<&str>::None;

            if let Some(allocator_entrypoint) = module_exports.get(cstr!(b"__popcorn_module_main_allocator")) {
                testing_fn = allocator_entrypoint.value.get();
            }
            if let Some(symbol) = module_exports.get(cstr!(b"__popcorn_module_author")) {
                let author_data = module.data_at_address(symbol.value).unwrap();
                let author_data = unsafe { &*slice_from_raw_parts(author_data, symbol.size.try_into().unwrap()) };
                author = core::str::from_utf8(author_data).map_err(|_| ModuleLoadError::InvalidAuthorMetadata)?;
            }
            if let Some(symbol) = module_exports.get(cstr!(b"__popcorn_module_modulename")) {
                let name_data = module.data_at_address(symbol.value).unwrap();
                let name_data = unsafe { &*slice_from_raw_parts(name_data, symbol.size.try_into().unwrap()) };
                name = Some(core::str::from_utf8(name_data).map_err(|_| ModuleLoadError::InvalidNameMetadata)?);
            }
            if let Some(symbol) = module_exports.get(cstr!(b"__popcorn_module_modulefqn")) {
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
    }

    // map framebuffer
    let framebuffer_info: Option<handoff::Framebuffer> = try {
        use uefi::proto::console::gop::PixelBitmask;

        let mode_info = gop.current_mode_info();
        let mut framebuffer_info = gop.frame_buffer();
        let (width, height) = mode_info.resolution();

        let page_count = (framebuffer_info.size() + PAGE_SIZE - 1) / PAGE_SIZE;
        kernel_first_page -= page_count * PAGE_SIZE;
        let fb_start = kernel_first_page;

        let framebuffer_addr = framebuffer_info.as_mut_ptr() as usize;

        for offset in (0..page_count).map(|num| num * PAGE_SIZE) {
            let virtual_addr = fb_start + offset;
            let physical_addr = framebuffer_addr + offset;
            kernel_page_table.try_map_page_with(
                Page(virtual_addr.try_into().unwrap()),
                Frame(physical_addr.try_into().unwrap()),
                page_table_allocator_fn,
                TableEntryFlags::WRITABLE | TableEntryFlags::NO_EXECUTE | TableEntryFlags::MMIO
            ).ok()?;
        }

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
            buffer: fb_start as *mut u8,
            stride: mode_info.stride(),
            width,
            height,
            color_format
        }
    };

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
            kernel_page_table.try_map_page(Page(addr), Frame(addr), page_table_allocator_fn)
        }).unwrap();
    }

    debug!("Generating kernel memory map");
    let kernel_mem_map = {
        let descriptor_to_entry = |descriptor: &MemoryDescriptor| -> MemoryMapEntry {
            use handoff::MemoryType::*;

            let mut ty = match descriptor.ty {
                MemoryType::CONVENTIONAL |
                MemoryType::BOOT_SERVICES_CODE |
                MemoryType::BOOT_SERVICES_DATA |
                MemoryType::RUNTIME_SERVICES_CODE |
                MemoryType::RUNTIME_SERVICES_DATA |
                MemoryType::PERSISTENT_MEMORY => Free,
                MemoryType::LOADER_CODE => BootloaderCode,
                MemoryType::LOADER_DATA => BootloaderData,
                memory_types::KERNEL_CODE => KernelCode,
                memory_types::PAGE_TABLE => KernelPageTable,
                memory_types::MODULE_CODE => ModuleCode,
                MemoryType::ACPI_NON_VOLATILE => AcpiPreserve,
                MemoryType::ACPI_RECLAIM => AcpiReclaim,
                _ => Reserved
            };

            if (descriptor.phys_start..descriptor.phys_start + descriptor.page_count * 4096).contains(&stack_ptr) {
                ty = KernelStack;
            }

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
        last_item.map(|item| kernel_mem_map.push(item));
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
            page_table_root: kernel_page_table.into()
        },
        modules: handoff::Modules {
	        phys_allocator_start: unsafe { mem::transmute(testing_fn) }
        },
        log: handoff::Logging,
        test: handoff::Testing {
            module_func: unsafe { mem::transmute(testing_fn) }
        }
    };

    type KernelStart = ffi_abi!(type fn(handoff::Data) -> !);
    let kernel_entry: KernelStart = unsafe { mem::transmute(kernel_entry) };
    kernel_entry(handoff);
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
}

#[derive(Display)]
enum ModuleLoadError {
    #[display(fmt = "Could not locate requested module")]
    FileNotFound,
    #[display(fmt = "Module file is corrupted")]
    InvalidElf,
    #[display(fmt = "Failed to resolve symbol {:?}", _0)]
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
