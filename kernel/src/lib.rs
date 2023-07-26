#![feature(custom_test_frameworks)]
#![test_runner(tests::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(result_option_inspect)]
#![feature(const_trait_impl)]
#![feature(pointer_byte_offsets)]
#![feature(allocator_api)]
#![no_std]
#![no_main]

extern crate alloc;

pub mod sync;
pub mod io;

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
			unsafe { asm!("in al, dx", in("dx") self.0, out("al") ret, options(nostack, preserves_flags)); }
			ret
		}

		#[inline(always)]
		pub unsafe fn write(&mut self, val: u8) {
			unsafe { asm!("out dx, al", in("dx") self.0, in("al") val, options(nostack, preserves_flags)); }
		}
	}

	impl Port<u16> {
		#[inline(always)]
		pub unsafe fn read(&self) -> u16 {
			let ret;
			unsafe { asm!("in ax, dx", in("dx") self.0, out("ax") ret, options(nostack, preserves_flags)); }
			ret
		}

		#[inline(always)]
		pub unsafe fn write(&mut self, val: u16) {
			unsafe { asm!("out dx, ax", in("dx") self.0, in("ax") val, options(nostack, preserves_flags)); }
		}
	}

	impl Port<u32> {
		#[inline(always)]
		pub unsafe fn read(&self) -> u32 {
			let ret;
			unsafe { asm!("in eax, dx", in("dx") self.0, out("eax") ret, options(nostack, preserves_flags)); }
			ret
		}

		#[inline(always)]
		pub unsafe fn write(&mut self, val: u32) {
			unsafe { asm!("out dx, eax", in("dx") self.0, in("eax") val, options(nostack, preserves_flags)); }
		}
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

