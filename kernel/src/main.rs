#![feature(custom_test_frameworks)]
#![test_runner(tests::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(result_option_inspect)]
#![feature(const_trait_impl)]
#![feature(pointer_byte_offsets)]
#![feature(lang_items)]
#![feature(allocator_api)]
#![no_std]
#![no_main]

extern crate alloc;
#[cfg(panic = "unwind")]
extern crate unwinding;

use core::alloc::{GlobalAlloc, Layout};
use core::cell::{Cell, RefCell};
use crate::io::serial;
use crate::io::serial::SERIAL0;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::slice_from_raw_parts_mut;
use core::sync::atomic::{AtomicUsize, Ordering};
use kernel_exports::memory::PhysicalAddress;
use kernel_exports::sync::Mutex;
use crate::memory::Allocator;
use crate::memory::watermark_allocator::WatermarkAllocator;

mod sync;
mod io;
mod memory;
mod panicking;

#[macro_export]
macro_rules! usize {
    ($stuff:expr) => {usize::try_from($stuff).unwrap()};
}

#[macro_export]
macro_rules! u64 {
    ($stuff:expr) => {u64::try_from($stuff).unwrap()};
}

#[macro_export]
macro_rules! into {
    ($stuff:expr) => {($stuff).try_into().unwrap()};
}

#[export_name = "_start"]
extern "sysv64" fn kstart(handoff_data: utils::handoff::Data) -> ! {
	serial::init_serial0().expect("Failed to initialise serial0");
	sprintln!("Hello world!");

	#[cfg(test)] {
		test_main();
		loop {}
	}
	#[cfg(not(test))] kmain(handoff_data)
}

fn kmain(mut handoff_data: utils::handoff::Data) -> ! {
	sprintln!("Handoff data:\n{handoff_data:x?}");

	/*let mut wmark = WatermarkAllocator::new(&mut handoff_data.memory.map);
	// Split allocator system is used when a significant portion of memory is above the 4GiB boundary
	// This allows better optimization for non-DMA allocations as well as reducing pressure on memory usable by DMA
	// The current algorithm uses split allocators when the total amount of non-DMA memory is >= 1GiB
	let split_allocators = if cfg!(target_pointer_width = "64") {
		use utils::handoff::MemoryType;
		const FOUR_GB: PhysicalAddress = PhysicalAddress(1<<32);

		let bytes_over_4gb: usize = handoff_data.memory.map.iter().filter(|entry|
			entry.ty == MemoryType::Free
			|| entry.ty == MemoryType::AcpiReclaim
			|| entry.ty == MemoryType::BootloaderCode
			|| entry.ty == MemoryType::BootloaderData
		)
				.filter(|entry| entry.start() >= FOUR_GB)
				.map(|entry| entry.end() - entry.start())
				.sum();

		bytes_over_4gb >= 1024*1024*1024
	} else { false };
	sprintln!("Split allocator: {}", if split_allocators { "enabled" } else { "disabled" });

	let map = unsafe { handoff_data.log.symbol_map.map(|ptr| ptr.as_ref()) };
	*panicking::SYMBOL_MAP.write().unwrap() = map;

	let low_mem_allocator = &wmark;
	let high_mem_allocator: &dyn Allocator = if !split_allocators { low_mem_allocator } else {
		let high_mem_allocator: &dyn Allocator = /* todo */;
		high_mem_allocator.chain(low_mem_allocator)
	};

	/*unsafe {
		// SAFETY: unset a few lines below
		memory::alloc::phys::GLOBAL_ALLOCATOR.set_unchecked(&mut wmark);
	}
	let thingy = (handoff_data.modules.phys_allocator_start)(Range(Frame::new(PhysicalAddress(0)), Frame::new(PhysicalAddress(0x10000))));
	memory::alloc::phys::GLOBAL_ALLOCATOR.unset();*/

	if let Some(fb) = handoff_data.framebuffer {
		let size = fb.stride * fb.height;
		for pixel in unsafe { &mut *slice_from_raw_parts_mut(fb.buffer.cast::<u32>(), size) }.iter_mut() {
			*pixel = 0xeeeeee;
		}
	}

	loop {}
}

#[cfg(not(test))]
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	sprintln!("kernel {info}");
	panicking::do_panic()
}

#[no_mangle]
pub fn __popcorn_module_panic(info: &PanicInfo) -> ! {
	panic!("Panic from module: {info}");
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_alloc(layout: Layout) -> *mut u8 {
	alloc::alloc::alloc(layout)
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_dealloc(ptr: *mut u8, layout: Layout) {
	alloc::alloc::dealloc(ptr, layout)
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_alloc_zeroed(layout: Layout) -> *mut u8 {
	alloc::alloc::alloc_zeroed(layout)
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
	alloc::alloc::realloc(ptr, layout, new_size)
}

#[no_mangle]
pub unsafe extern "Rust" fn __popcorn_module_is_panicking() -> bool { panicking::panicking() }

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

#[global_allocator]
static Allocator: Foo = Foo(Mutex::new(FooInner {
	buffer: [0; 20],
	used: false,
}));

struct Foo(Mutex<FooInner>);

struct FooInner {
	buffer: [u64; 20],
	used: bool
}

unsafe impl GlobalAlloc for Foo {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let mut this = self.0.lock().unwrap();
		if this.used { core::ptr::null_mut() }
		else if layout.size() > (this.buffer.len() * 8) || layout.align() > 8 { core::ptr::null_mut() }
		else {
			this.used = true;
			(&mut this.buffer).as_mut_ptr().cast()
		}
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		self.0.lock().unwrap().used = false;
	}

	/*unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		todo!()
	}*/
}

#[cfg(test)]
mod tests {
	use core::panic::PanicInfo;
	use macros::test_should_panic;
	use crate::{panicking::do_panic, panicking, sprint, sprintln};

	pub enum Result { Success, Fail }

	pub trait Testable {
		fn run(&self) -> Result;
	}

	impl<T> Testable for T where T: Fn() {
		fn run(&self) -> Result {
			sprint!("{}...\t", core::any::type_name::<T>());
			match panicking::catch_unwind(self) {
				Ok(_) => { sprintln!("[ok]"); Result::Success },
				Err(_) => { sprintln!("[FAIL]"); Result::Fail }
				// todo: print panic message
			}
		}
	}

	pub fn test_runner(tests: &[&dyn Testable]) -> ! {
		sprintln!("Running {} tests", tests.len());
		let mut success_count = 0;
		for test in tests {
			match test.run() {
				Result::Success => success_count += 1,
				Result::Fail => {}
			}
		}

		sprintln!("\nTest result: {}. {} passed; {} failed",
			if success_count == tests.len() { "ok" } else { "fail" },
			success_count,
			tests.len() - success_count
		);
		loop {}
		//todo!("Exit qemu");
	}

	#[panic_handler]
	fn panic_handler(info: &PanicInfo) -> ! {
		sprintln!("{info}");
		do_panic()
	}

	#[test_case]
	fn trivial_assertion() {
		assert_eq!(1, 1);
	}

	#[test_should_panic]
	fn should_panic() {
		assert_eq!(1, 2);
	}
}

