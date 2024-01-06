#![unstable(feature = "kernel_virtual_memory", issue = "none")]

use auto_impl::auto_impl;
use crate::memory::Page;
use super::AllocError;

#[auto_impl(&, Box, Arc)]
pub trait VirtualAllocator: Send + Sync {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError>;
	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError>;
	fn deallocate_contiguous(&self, base: Page, len: usize);
}

pub struct Global;

extern "Rust" {
	#[link_name = "__popcorn_memory_virtual_kernel_global"]
	static GLOBAL_VIRTUAL_ALLOCATOR: dyn* VirtualAllocator;
}

impl VirtualAllocator for Global {
	#[track_caller]
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.allocate_contiguous(len)
	}

	#[track_caller]
	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.allocate_contiguous_at(at, len)
	}

	#[track_caller]
	fn deallocate_contiguous(&self, base: Page, len: usize) {
		unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.deallocate_contiguous(base, len)
	}
}
