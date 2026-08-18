#![feature(try_blocks)]
#![feature(arbitrary_self_types)]
#![feature(step_trait)]
#![feature(error_iter)]
#![feature(duration_integer_division)]
#![no_main]
#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use core::convert::Infallible;
use core::error::Error;
use core::panic::PanicInfo;
use core::time::Duration;
use log::{debug, error, info};
use uefi::{entry, println, Status, boot};

//mod paging;
mod logging;
//mod elf;
mod fs;
mod loader;

const PAGE_MAP_OFFSET: u64 = 0xffff_8000_0000_0000;
const PAGE_MAP_OFFSET_LEN: u64 = 2u64.pow(46);

#[entry]
fn bootloader_entry() -> Status {
    let error = main().expect_err("Ok path will never return");
    error!("{}", error);
    for source in error.sources() {
        error!(target: "<continuation>", "Caused by: {}", source);
    }

	boot::stall(Duration::from_secs(10));

    Status::ABORTED
}

fn main() -> Result<Infallible, Box<dyn Error>> {
    println!("Loading popcorn2...");

	logging::init()?;

	let mut rootfs = fs::locate_rootfs()?;
	let version = loader::find_latest_kernel(&mut *rootfs)?;
	info!("Booting kernel {version}");

	loop {}
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
