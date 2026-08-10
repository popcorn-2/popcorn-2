//! A cross-platform UEFI bootloader for loading Popcorn2.

#![feature(arbitrary_self_types)]
#![feature(error_iter)]
#![feature(duration_integer_division)]
#![no_main]
#![no_std]

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
use uefi::mem::memory_map::{MemoryMap, MemoryMapMut};
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
        error!(target: "<continuation>", "Caused by: {}", source);
    }

	debug!("{error:?}");

	boot::stall(Duration::from_secs(10));

    Status::ABORTED
}

fn main() -> Result<Infallible, Box<dyn Error>> {
    println!("Loading popcorn2...");

	logging::init()?;

	let mut rootfs = fs::locate_rootfs()?;
	let version = fs::find_latest_kernel(&mut *rootfs)?;
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
		mapper.new_mapping(
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
		mapper.new_mapping(
			Some(base),
			Some(base_virt),
			entry.page_count as usize,
			Ty::LOADER_CODE,
		)?;
	}

	info!("setting up system for kernel entry");

	arch::final_init();
	let (page_table, entry, stack, lowest_used) = mapper.finalize();

	let rsdp = system::with_config_table(|config_tables| {
		if let Some(xsdp) = config_tables.iter().find(|table| table.guid == ConfigTableEntry::ACPI2_GUID) {
			PhysicalAddress::new(xsdp.address as usize)
		} else if let Some(rsdp) = config_tables.iter().find(|table| table.guid == ConfigTableEntry::ACPI_GUID) {
			PhysicalAddress::new(rsdp.address as usize)
		} else {
			panic!("No RSDP found");
		}
	});

	let mut handoff = handoff::Data {
		framebuffer: Some(framebuffer),
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

	const {
		// check that it's valid to overwrite the array in-place
		assert!(size_of::<handoff::MemoryMapEntry>() <= size_of::<boot::MemoryDescriptor>());
		assert!(align_of::<handoff::MemoryMapEntry>() <= align_of::<boot::MemoryDescriptor>());
	}

	let mut map = unsafe { boot::exit_boot_services(None) };
	map.sort();
	let len = map.len();
	let mut buffer = unsafe { map.buffer_mut() };
	for i in 0..len {
		let entry = unsafe { &*buffer.as_ptr().cast::<boot::MemoryDescriptor>().add(i) };
		let start = PhysicalAddress::new(entry.phys_start as usize);
		let entry = handoff::MemoryMapEntry {
			coverage: Range { start, end: start + entry.page_count as usize },
			ty: match entry.ty {
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
		unsafe { buffer.as_mut_ptr().cast::<handoff::MemoryMapEntry>().write(entry) };
		buffer = &mut buffer[size_of::<handoff::MemoryMapEntry>()..];
	}

	handoff.memory.map = unsafe {
		slice::from_raw_parts(
			buffer.as_ptr().cast::<handoff::MemoryMapEntry>(),
			len,
		)
	};

	page_table.switch();
	arch::handover(entry, *(stack.bottom_virt + stack.page_count), &handoff)
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

	pub fn kernel_seg_to_mapping_ty(ty: Type, flags: Flags) -> Ty {
		match ty {
            Type::LOAD if flags.contains(Flags::Executable) => Ty::KERNEL_CODE,
            Type::LOAD => Ty::KERNEL_DATA,
            Type::TLS => Ty::KERNEL_TLS,
			_ => Ty::KERNEL_OTHER,
		}
	}
}
