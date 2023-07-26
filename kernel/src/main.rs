#![no_std]
#![no_main]

extern crate alloc;

use alloc::alloc::alloc;
use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use log::{debug, info, warn};
use kernel::io::serial;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::slice_from_raw_parts_mut;
use kernel::io::serial::SERIAL0;
//use kernel::memory;
//use kernel::memory::watermark_allocator::WatermarkAllocator;
use kernel_exports::memory::{Frame, PhysicalAddress, PhysicalMemoryAllocator};
use utils::handoff::Range;

#[export_name = "_start"]
extern "sysv64" fn kstart(handoff_data: utils::handoff::Data) -> ! {
	serial::init_serial0().expect("Failed to initialise serial0");
	writeln!(SERIAL0.lock(), "Hello world!").unwrap();

	//#[cfg(test)] test_main();
	#[cfg(not(test))] kmain(/*handoff_data*/)
}

fn kmain(mut handoff_data: utils::handoff::Data) -> ! {
	writeln!(SERIAL0.lock(), "Handoff data:\n{handoff_data:x?}").unwrap();

	/*let mut wmark = WatermarkAllocator::new(&mut handoff_data.memory.map);

	unsafe {
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
	let _ = writeln!(SERIAL0.lock(), "{info}");
	loop {

	}
}

#[no_mangle]
pub fn __popcorn_module_panic(info: &PanicInfo) -> ! {
	let _ = writeln!(SERIAL0.lock(), "Panic from module: {info}");
	loop {

	}
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

#[global_allocator]
static Allocator: Foo = Foo;

struct Foo;
unsafe impl GlobalAlloc for Foo {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		todo!()
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		todo!()
	}

	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
		todo!()
	}
}
