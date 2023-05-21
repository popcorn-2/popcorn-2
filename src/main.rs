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

use log::{debug, info, warn};
use memory::PhysAddr;
use io::serial;
use core::fmt::Write;
use core::panic::PanicInfo;

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

	#[derive(Debug, Copy, Clone)]
	pub struct Port<T>(u16, PhantomData<T>) where T: PortWidth;

	impl<T> Port<T> where T: PortWidth {
		pub const fn new(addr: u16) -> Self {
			Self(addr, PhantomData)
		}
	}

	impl Port<u8> {
		#[inline(always)]
		pub unsafe fn read(&self) -> u8 {
			let ret;
			unsafe { asm!("in al, dx", in("dx") self.0, out("al") ret); }
			ret
		}

		#[inline(always)]
		pub unsafe fn write(&mut self, val: u8) {
			unsafe { asm!("out dx, al", in("dx") self.0, in("al") val); }
		}
	}

	impl Port<u16> {
		#[inline(always)]
		pub unsafe fn read(&self) -> u16 {
			let ret;
			unsafe { asm!("in ax, dx", in("dx") self.0, out("ax") ret); }
			ret
		}

		#[inline(always)]
		pub unsafe fn write(&mut self, val: u16) {
			unsafe { asm!("out dx, ax", in("dx") self.0, in("ax") val); }
		}
	}

	impl Port<u32> {
		#[inline(always)]
		pub unsafe fn read(&self) -> u32 {
			let ret;
			unsafe { asm!("in eax, dx", in("dx") self.0, out("eax") ret); }
			ret
		}

		#[inline(always)]
		pub unsafe fn write(&mut self, val: u32) {
			unsafe { asm!("out dx, eax", in("dx") self.0, in("eax") val); }
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
