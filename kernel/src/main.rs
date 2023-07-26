#![no_std]
#![no_main]

use alloc::alloc::alloc;
use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use log::{debug, info, warn};
use kernel::io::serial;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::slice_from_raw_parts_mut;
use kernel::io::serial::SERIAL0;
#[export_name = "_start"]
	serial::init_serial0().expect("Failed to initialise serial0");
	writeln!(SERIAL0.lock(), "Hello world!").unwrap();

	//#[cfg(test)] test_main();
	#[cfg(not(test))] kmain(/*handoff_data*/)
}

fn kmain(/*mut handoff_data: utils::handoff::Data*/) -> ! {
	loop {}
}

#[cfg(not(test))]
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	loop {

	}
}
