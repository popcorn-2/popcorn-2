#![no_std]

#![feature(allocator_api)]
#![feature(kernel_mmap_to_parts)]
#![feature(let_chains)]

use core::alloc::{AllocError, Layout};
use core::ptr::NonNull;
use kernel_api::memory::mapping::Mapping;

mod chunk;
mod arena;
mod mapped_vec;

#[unsafe(no_mangle)]
pub extern "Rust" fn __popcorn_kernel_heap_allocate(layout: Layout) -> Result<NonNull<u8>, AllocError> {
	todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_deallocate(ptr: NonNull<u8>, layout: Layout)  {
	todo!()
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_reallocate(ptr: NonNull<u8>, layout: Layout, new_size: usize) -> Result<NonNull<u8>, AllocError>  {
	todo!()
}
