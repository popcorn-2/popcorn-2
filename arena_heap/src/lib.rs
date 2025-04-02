#![no_std]

#![feature(allocator_api)]

use core::alloc::{AllocError, Layout};
use core::ptr::NonNull;

#[unsafe(no_mangle)]
pub extern "Rust" fn __popcorn_kernel_heap_allocate(layout: Layout) -> Result<NonNull<u8>, AllocError> {
	todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_deallocate(ptr: NonNull<u8>, layout: Layout)  {
	todo!()
}
