#![unstable(feature = "kernel_virtual_memory", issue = "none")]

use core::mem::ManuallyDrop;
use core::num::NonZeroUsize;
use core::ptr;
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

// todo: can this be macroed?
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

pub struct OwnedPages<A: VirtualAllocator = Global> {
	base: Page,
	len: NonZeroUsize,
	allocator: A
}

impl OwnedPages<Global> {
	pub fn new(len: NonZeroUsize) -> Result<Self, AllocError> {
		let base = Global.allocate_contiguous(len.get())?;
		Ok(Self {
			base,
			len,
			allocator: Global
		})
	}
}

impl<A: VirtualAllocator> OwnedPages<A> {
	pub fn new_with(len: NonZeroUsize, allocator: A) -> Result<Self, AllocError> {
		let base = allocator.allocate_contiguous(len.get())?;
		Ok(Self {
			base,
			len,
			allocator
		})
	}

	pub fn into_raw_parts(self) -> (Page, NonZeroUsize, A) {
		let me = ManuallyDrop::new(self);
		(
			me.base,
			me.len,
			unsafe { ptr::read(&me.allocator) }
		)
	}

	pub unsafe fn from_raw_parts(base: Page, len: NonZeroUsize, allocator: A) -> Self {
		Self {
			base, len, allocator
		}
	}
}

impl<A: VirtualAllocator> Drop for OwnedPages<A> {
	fn drop(&mut self) {
		self.allocator.deallocate_contiguous(self.base, self.len.get());
	}
}
