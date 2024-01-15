#![unstable(feature = "kernel_internals", issue = "none")]

use core::num::NonZeroUsize;
use crate::memory::allocator::BackingAllocator;
use crate::memory::Frame;
use crate::memory::mapping::Highmem;

/// Conceptually the same as a hypothetical `Arc<[Frame]>`
///
/// # Invariants
///
/// Internal implementation assumes that two `OwnedFrames` cannot overlap unless they came from the same original `OwnedFrames`
/// object
pub struct OwnedFrames<'allocator> {
	base: Frame,
	len: NonZeroUsize,
	allocator: &'allocator dyn BackingAllocator
}

impl<'a> OwnedFrames<'a> {
	fn split_at(self, n: NonZeroUsize) -> (Self, Self) {
		assert!(n <= self.len);

		/*let second_base = self.base + n.get();
		let lens = (n, self.len - n);*/

		todo!("Insert new RC node")

		//(Self { base: self.base, len: lens.0 }, Self { base: second_base, len: lens.1 })
	}

	pub fn into_raw_parts(self) -> (Frame, NonZeroUsize, &'a dyn BackingAllocator) {
		(self.base, self.len, self.allocator)
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
