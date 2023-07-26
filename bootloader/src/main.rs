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
use uefi::data_types::{Align, Identify, PhysicalAddress};
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

    let Ok(gop_handle) = services.get_handle_for_protocol::<GraphicsOutput>() else {
        //let _ = system_table.stderr().output_string(cstr16!("Unable to enable graphics")); // Can't really do anything if this fails
        return Status::PROTOCOL_ERROR;
    };

    let Ok(mut gop) = (unsafe {
        services.open_protocol::<GraphicsOutput>(OpenProtocolParams {
            handle: gop_handle,
            agent: services.image_handle(),
            controller: None,
        }, OpenProtocolAttributes::GetProtocol)
    }) else {
        //let _ = system_table.stderr().output_string(cstr16!("Unable to enable graphics")); // Can't really do anything if this fails
        return Status::PROTOCOL_ERROR;
    };

    let Ok(uart) = services.get_handle_for_protocol::<Serial>() else {
        //let _ = system_table.stderr().output_string(cstr16!("Unable to enable graphics")); // Can't really do anything if this fails
        return Status::PROTOCOL_ERROR;
    };

    let Ok(mut uart) = services.open_protocol_exclusive::<Serial>(uart) else {
        //let _ = system_table.stderr().output_string(cstr16!("Unable to enable graphics")); // Can't really do anything if this fails
        return Status::PROTOCOL_ERROR;
    };

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

    let config = fs.read_to_string(Path::new(cstr16!(r"EFI\POPCORN\config.toml"))).unwrap();
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
    unsafe { logging::init(&mut ui, uart.deref_mut()).unwrap(); }

    if let Ok(edid_handle) = services.get_handle_for_protocol::<framebuffer::ActiveEdid>()
	    && let Ok(edid) = services.open_protocol_exclusive::<framebuffer::ActiveEdid>(edid_handle) {
            info!("EDID: {:?}", edid.deref().deref());
        } else {
            warn!("Could not get EDID info");
        }


        // =========== test code using kernel from efi part ===========

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
    loop {}
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
