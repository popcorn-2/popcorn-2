//! A cross-platform UEFI bootloader for loading Popcorn2.

#![feature(arbitrary_self_types)]
#![feature(error_iter)]
#![feature(duration_integer_division)]
#![feature(strict_provenance_lints)]
#![cfg_attr(doc, feature(rustdoc_missing_doc_code_examples))]
#![no_main]
#![no_std]

#![expect(clippy::cast_possible_truncation, reason = "needs thought and rework of dependencies")]

extern crate alloc;

use alloc::boxed::Box;
use core::convert::Infallible;
use core::error::Error;
use core::panic::PanicInfo;
use core::range::Range;
use core::slice;
use core::time::Duration;
use log::{debug, error, info};
use uefi::{entry, println, Status, boot, system};
use uefi::boot::MemoryType;
use uefi::mem::memory_map::{MemoryMap as _, MemoryMapMut as _, MemoryMapOwned};
use uefi::table::cfg::ConfigTableEntry;
use kernel_api::mapping::Ty;
use kernel_api::memory::{PhysicalAddress, RawFrame, RawPage};
use utils::handoff;
use crate::mapper::Mapper;

mod framebuffer;
mod logging;
mod mapper;
mod fs;
mod arch;

#[entry]
fn bootloader_entry() -> Status {
    let error = main().expect_err("Ok path will never return");
    error!("{error}");
    for source in error.sources() {
        error!(target: "<continuation>", "Caused by: {source}");
    }

	debug!("{error:?}");

	boot::stall(Duration::from_secs(10));

    Status::ABORTED
}

/// # Errors
///
/// Returns an error for any failure that prevented the kernel from starting.
///
/// # Panics
///
/// If the system environment or firmware prevents a boot finishing.
fn main() -> Result<Infallible, Box<dyn Error>> {
    println!("Loading popcorn2...");

	logging::init()?;

	let mut rootfs = fs::locate_rootfs()?;
	let version = fs::find_latest_kernel(&mut rootfs)?;
	info!("Booting kernel {version}");
	let kernel = fs::unpack_kernel(rootfs, version)?;
	let mut mapper = Mapper::try_new(kernel)?;

	let framebuffer = framebuffer::map_framebuffer(&mut mapper)?;

	debug!("mapping page map region");
	let mem_map = boot::memory_map(MemoryType::LOADER_DATA)?;
	for entry in mem_map.entries().filter(|entry|
		entry.ty == MemoryType::BOOT_SERVICES_CODE ||
		entry.ty == MemoryType::BOOT_SERVICES_DATA ||
		entry.ty == MemoryType::PERSISTENT_MEMORY ||
		entry.ty == MemoryType::LOADER_CODE ||
		entry.ty == MemoryType::LOADER_DATA ||
		entry.ty == MemoryType::ACPI_NON_VOLATILE ||
		entry.ty == MemoryType::ACPI_RECLAIM ||
		entry.ty == MemoryType::RUNTIME_SERVICES_CODE ||
		entry.ty == MemoryType::RUNTIME_SERVICES_DATA ||
		entry.ty == MemoryType::CONVENTIONAL
	) {
		// fixme: conversion
		let base = RawFrame::new(entry.phys_start as usize);
		let _ = mapper.new_mapping(
			Some(base),
			Some(base.to_virtual().align_down_to_page()),
			entry.page_count as usize,
			Ty::MEM_MAP,
		)?;
	}

	debug!("identity mapping bootloader");
	for entry in mem_map.entries().filter(|entry|
		entry.ty == MemoryType::LOADER_CODE ||
		entry.ty == MemoryType::LOADER_DATA
	) {
		// fixme: conversion
		let base = RawFrame::new(entry.phys_start as usize);
		let base_virt = RawPage::new(entry.phys_start as usize);
		let _ = mapper.new_mapping(
			Some(base),
			Some(base_virt),
			entry.page_count as usize,
			Ty::LOADER_CODE,
		)?;
	}

	info!("setting up system for kernel entry");

	arch::final_init();
	let (page_table, entry, stack, lowest_used, used_frames) = mapper.finalize();

	let rsdp = system::with_config_table(|config_tables| {
		#[expect(clippy::option_if_let_else, reason = "more easily readable")]
		if let Some(xsdp) = config_tables.iter().find(|table| table.guid == ConfigTableEntry::ACPI2_GUID) {
			PhysicalAddress::new(xsdp.address.addr())
		} else if let Some(rsdp) = config_tables.iter().find(|table| table.guid == ConfigTableEntry::ACPI_GUID) {
			PhysicalAddress::new(rsdp.address.addr())
		} else {
			panic!("No RSDP found");
		}
	});

	let mut handoff = handoff::Data {
		framebuffer,
		memory: handoff::Memory {
			map: &[],
			lowest_used,
			stack,
		},
		log: handoff::Logging {
			symbol_map: None,
		},
		rsdp,
		init_exec: &[],
		ramdisk: &[],
	};

	// SAFETY: only actions that happen after exiting boot services are switching
	//  page tables and jumping to kernel
	let map = unsafe { boot::exit_boot_services(None) };
	// SAFETY: boot services just exited
	handoff.memory.map = unsafe { convert_mem_map(map, used_frames) };

	// SAFETY: all bootloader data is mapped to same location
	unsafe { page_table.switch() };
	arch::handover(entry, *(stack.bottom_virt + stack.page_count), &raw const handoff)
}

// When exiting boot services, the firmware generates a final meomry map.
// This needs converting to pass to the kernel, but we can't allocate any
// new memory since boot services has been exited.
// As long as each kernel entry is smaller than each UEFI entry, the UEFI buffer
// is suitably aligned for kernel entries, and we work from one end to the other,
// never reading the section we've already replaced, then the entire memory map
// can be converted in place.
/// # Safety
///
/// This must only be called after boot services has been exited.
unsafe fn convert_mem_map(mut map: MemoryMapOwned, kernel_frames: &[RawFrame]) -> &'static [handoff::MemoryMapEntry] {
	const {
		// check that it's valid to overwrite the array in-place
		assert!(size_of::<handoff::MemoryMapEntry>() <= size_of::<boot::MemoryDescriptor>(), "UEFI descriptor too small");
		assert!(align_of::<handoff::MemoryMapEntry>() <= align_of::<boot::MemoryDescriptor>(), "buffer too low alignment");
	}

	map.sort();

	let len = map.len();
	// SAFETY: `map` is only accessed through `buffer`
	let mut buffer = unsafe { map.buffer_mut() };
	#[expect(clippy::cast_ptr_alignment, reason = "buffer was allocated as aligned by firmware")]
	let buffer_start = buffer.as_ptr().cast::<boot::MemoryDescriptor>();

	for i in 0..len {
		// SAFETY: `buffer` is large enough to hold `len` elements of `boot::MemoryDescriptor`
		let entry = unsafe { &*buffer_start.add(i) };
		let start = PhysicalAddress::new(entry.phys_start as usize);
		let entry = handoff::MemoryMapEntry {
			coverage: Range { start, end: start + entry.page_count as usize },
			ty: match entry.ty {
				_ if kernel_frames.contains(&RawFrame::new(entry.phys_start as usize)) => handoff::MemoryType::KernelData,
				MemoryType::CONVENTIONAL |
				MemoryType::BOOT_SERVICES_CODE |
				MemoryType::BOOT_SERVICES_DATA |
				MemoryType::PERSISTENT_MEMORY => handoff::MemoryType::Free,
				MemoryType::LOADER_CODE => handoff::MemoryType::BootloaderCode,
				MemoryType::LOADER_DATA => handoff::MemoryType::BootloaderData,
				MemoryType::ACPI_NON_VOLATILE => handoff::MemoryType::AcpiPreserve,
				MemoryType::ACPI_RECLAIM => handoff::MemoryType::AcpiReclaim,
				MemoryType::RUNTIME_SERVICES_CODE => handoff::MemoryType::RuntimeCode,
				MemoryType::RUNTIME_SERVICES_DATA => handoff::MemoryType::RuntimeData,
				_ => handoff::MemoryType::Reserved
			}
		};

		// SAFETY: pointer was acquired from a mut reference
		unsafe {
			#[expect(clippy::cast_ptr_alignment, reason = "manually checked alignment")]
			buffer.as_mut_ptr().cast::<handoff::MemoryMapEntry>().write(entry);
		}
		buffer = &mut buffer[size_of::<handoff::MemoryMapEntry>()..];
	}

	// SAFETY: creating from data we just initialised, and boot services has been exited
	//  so the buffer is leaked and therefore 'static
	unsafe {
		slice::from_raw_parts(
			buffer_start.cast::<handoff::MemoryMapEntry>(),
			len,
		)
	}
}

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    error!("{info}");

	loop {} // watchdog timer should kill us after long enough
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
	use elf::segment::{Flags, Type};
	use kernel_api::mapping::Ty;

	pub const fn kernel_seg_to_mapping_ty(ty: Type, flags: Flags) -> Ty {
		match ty {
            Type::LOAD if flags.contains(Flags::Executable) => Ty::KERNEL_CODE,
            Type::LOAD => Ty::KERNEL_DATA,
            Type::TLS => Ty::KERNEL_TLS,
			_ => Ty::KERNEL_OTHER,
		}
	}
}
