#![unstable(feature = "kernel_virtual_memory", issue = "none")]

use alloc::sync::Arc;
use core::fmt::{Debug, Formatter};
use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::ptr;
use core::ptr::NonNull;
use auto_impl::auto_impl;
use log::debug;
use crate::bridge::paging::AddressSpaceInner;
use crate::memory::Page;
use crate::sync::RwSpinlock;
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
	static GLOBAL_VIRTUAL_ALLOCATOR: RwSpinlock<&'static dyn VirtualAllocator>;
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
	len: NonZero<usize>,
	allocator: A
}

impl OwnedPages<Global> {
	pub fn new(len: NonZero<usize>) -> Result<Self, AllocError> {
		let base = Global.allocate_contiguous(len.get())?;
		Ok(Self {
			base,
			len,
			allocator: Global
		})
	}
}

impl<A: VirtualAllocator> OwnedPages<A> {
	pub fn new_with(len: NonZero<usize>, allocator: A) -> Result<Self, AllocError> {
		let base = allocator.allocate_contiguous(len.get())?;
		Ok(Self {
			base,
			len,
			allocator
		})
	}

	pub fn into_raw_parts(self) -> (Page, NonZero<usize>, A) {
		let this = ManuallyDrop::new(self);
		(
			this.base,
			this.len,
			unsafe { ptr::read(&this.allocator) }
		)
	}

	pub unsafe fn from_raw_parts(base: Page, len: NonZero<usize>, allocator: A) -> Self {
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

pub struct AddressSpace(#[unstable(feature = "kernel_internals", issue = "none")] pub Arc<AddressSpaceInner>);

impl AddressSpace {
	pub fn as_ptr(&self) -> NonNull<AddressSpaceInner> {
		// SAFETY: pointer returned by Arc must be non-null
		unsafe { NonNull::new_unchecked(Arc::as_ptr(&self.0).cast_mut()) }
	}
	
	/// # Safety
	/// 
	/// The AddressSpace must stay alive until a new AddressSpace gets loaded
	pub unsafe fn load(&self) {
		unsafe { crate::bridge::paging::__popcorn_address_space_load(&self.0); }
	}
}

impl Debug for AddressSpace {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("AddressSpace")
				.finish_non_exhaustive()
	}
}
