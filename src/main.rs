#![feature(result_option_inspect)]
#![feature(custom_test_frameworks)]
#![feature(const_trait_impl)]
#![test_runner(tests::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![no_std]
#![no_main]

mod memory;
mod sync;
mod io;

use core::panic::PanicInfo;
use log::{debug, error, info, warn};
use memory::PhysAddr;
use io::serial;
use core::fmt::Write;

#[no_mangle]
pub extern "C" fn kmain(multiboot_magic: u32, multiboot_addr: u32) {
	serial::init_serial0().expect("Failed to initialise serial0");
	write!(serial::SERIAL0.lock(), "Hello world!\n").unwrap();

	#[cfg(test)] test_main();
	#[cfg(not(test))] main(multiboot_magic, multiboot_addr);
}

fn main(multiboot_magic: u32, multiboot_addr: u32) {
	let multiboot_addr = PhysAddr::from(multiboot_addr);

	loop {}

	if multiboot_magic == 0x36d76289 {
		info!("Multiboot magic: 0x36d76289 (correct)");
	} else {
		warn!("Multiboot magic: {multiboot_magic:#x} (incorrect)");
	}

	debug!("Multiboot info struct loaded at {multiboot_addr:p}");
}

mod arch {
	use core::arch::asm;
	use core::marker::PhantomData;

	pub trait PortWidth {}
	impl PortWidth for u8 {}
	impl PortWidth for u16 {}
	impl PortWidth for u32 {}

	pub trait PortBacking: From<Self::Width> + Into<Self::Width> {
		type Width: PortWidth;
	}
	impl PortBacking for u8 {
		type Width = Self;
	}
	impl PortBacking for u16 {
		type Width = Self;
	}
	impl PortBacking for u32 {
		type Width = Self;
	}

	#[derive(Debug, Copy, Clone)]
	pub struct Port<T>(u16, PhantomData<T>) where T: PortBacking;

	impl<T> Port<T> where T: PortBacking {
		pub const fn new(addr: u16) -> Self {
			Self(addr, PhantomData)
		}
	}

	impl<T: PortBacking<Width = u8>> Port<T> {
		#[inline(always)]
		pub unsafe fn read(&self) -> T {
			let ret;
			unsafe { asm!("in al, dx", in("dx") self.0, out("al") ret); }
			T::from(ret)
		}

		#[inline(always)]
		pub unsafe fn write(&mut self, val: T) {
			let val = val.into();
			unsafe { asm!("out dx, al", in("dx") self.0, in("al") val); }
		}
	}
}

#[cfg(not(test))]
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	loop {

	}
}

#[cfg(test)]
mod tests {
	use core::panic::PanicInfo;
	use crate::{sprint, sprintln};

	pub trait Testable {
		fn run(&self);
	}

	impl<T> Testable for T where T: Fn() {
		fn run(&self) {
			sprint!("{}...\t", core::any::type_name::<T>());
			self();
			sprintln!("[ok]");
		}
	}

	pub fn test_runner(tests: &[&dyn Testable]) -> ! {
		sprintln!("Running {} tests", tests.len());
		for test in tests {
			test.run();
		}

		loop {}
		//todo!("Exit qemu");
	}

	#[panic_handler]
	fn panic_handler(info: &PanicInfo) -> ! {
		sprintln!("[failed]");
		sprintln!("Error: {info}");

		loop {

		}
		//todo!("Exit qemu");
	}

	#[test_case]
	fn _trivial_assertion() {
		assert_eq!(1, 1);
	}

	#[test_case]
	fn failing_test() {
		assert_eq!(1, 2);
	}
}
