#![unstable(feature = "kernel_internals", issue = "none")]

use core::mem::ManuallyDrop;
use core::num::NonZeroUsize;
use crate::memory::allocator::BackingAllocator;
use crate::memory::{allocator, AllocError, Frame};
use crate::memory::mapping::Highmem;
use crate::sync::{RwLock, RwReadGuard};

#[unstable(feature = "kernel_internals", issue = "none")]
pub struct GlobalAllocator {
}

#[unstable(feature = "kernel_internals", issue = "none")]
unsafe impl BackingAllocator for GlobalAllocator {
	fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, allocator::AllocError> {
		todo!()
	}

	unsafe fn deallocate_contiguous(&self, base: Frame, frame_count: NonZeroUsize) {
		todo!()
	}
}

#[unstable(feature = "kernel_internals", issue = "none")]
#[inline]
#[track_caller]
pub fn highmem() -> &'static GlobalAllocator {
	todo!()
}

#[unstable(feature = "kernel_internals", issue = "none")]
#[inline]
#[track_caller]
pub fn dmamem() -> &'static GlobalAllocator {
	todo!()
}

/// Conceptually the same as a hypothetical `Arc<[Frame]>`
///
/// # Invariants
///
/// Internal implementation assumes that two `OwnedFrames` cannot overlap unless they came from the same original `OwnedFrames`
/// object
pub struct OwnedFrames<'allocator> {
	pub(super) base: Frame,
	pub(super) len: NonZeroUsize,
	allocator: &'allocator dyn BackingAllocator
}

impl OwnedFrames<'static> {
	pub fn new(count: NonZeroUsize) -> Result<Self, AllocError> {
		Self::new_with(count, highmem())
	}
}

impl<'a> OwnedFrames<'a> {
	pub fn new_with(count: NonZeroUsize, allocator: &'a dyn BackingAllocator) -> Result<Self, AllocError> {
		let base = allocator.allocate_contiguous(count.get())?;
		Ok(OwnedFrames {
			base,
			len: count,
			allocator
		})
	}

	fn split_at(self, n: NonZeroUsize) -> (Self, Self) {
		assert!(n <= self.len);

		/*let second_base = self.base + n.get();
		let lens = (n, self.len - n);*/

		todo!("Insert new RC node")

		//(Self { base: self.base, len: lens.0 }, Self { base: second_base, len: lens.1 })
	}

	pub fn into_raw_parts(self) -> (Frame, NonZeroUsize, &'a dyn BackingAllocator) {
		let this = ManuallyDrop::new(self);
		(this.base, this.len, this.allocator)
	}

	pub unsafe fn from_raw_parts(base: Frame, len: NonZeroUsize, allocator: &'a dyn BackingAllocator) -> Self {
		Self {
			base, len, allocator
		}
	}
}

impl Clone for OwnedFrames<'_> {
	fn clone(&self) -> Self {
		todo!("Update RC nodes");
		Self { .. *self }
	}
}

impl Drop for OwnedFrames<'_> {
	fn drop(&mut self) {
		unsafe {
			self.allocator.deallocate_contiguous(self.base, self.len);
		}
	}
}
