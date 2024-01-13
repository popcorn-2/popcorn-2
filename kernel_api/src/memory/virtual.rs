#![unstable(feature = "kernel_virtual_memory", issue = "none")]

use auto_impl::auto_impl;
use log::debug;
use crate::memory::Page;
use crate::sync::{OnceLock, RwLock};
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
	static GLOBAL_VIRTUAL_ALLOCATOR: RwLock<&'static dyn VirtualAllocator>;
}

impl VirtualAllocator for Global {
	#[track_caller]
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		let at = unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.read().allocate_contiguous(len)?;
		debug!("Global VMA allocated at {at:x?}+{len}");
		Ok(at)
	}

	#[track_caller]
	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		debug!("Global VMA allocate contiguous {at:x?}+{len}");
		unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.read().allocate_contiguous_at(at, len)
	}

	#[track_caller]
	fn deallocate_contiguous(&self, base: Page, len: usize) {
		unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.read().deallocate_contiguous(base, len)
	}
}
