use core::alloc::AllocError;
use core::num::NonZeroUsize;
use kernel_exports::memory::Frame;
use super::Allocator;

pub struct ChainedAllocator<'a, 'b> {
	first: &'a dyn Allocator,
	second: &'b dyn Allocator
}

impl<'a, 'b> ChainedAllocator<'a, 'b> {
	pub(super) fn new(first: &'a dyn Allocator, second: &'b dyn Allocator) -> Self {
		Self { first, second }
	}
}

impl<'a, 'b> Allocator for ChainedAllocator<'a, 'b> {
	fn allocate_contiguous_aligned(&self, count: NonZeroUsize, alignment_log2: usize) -> Result<Frame, AllocError> {
		self.first.allocate_contiguous_aligned(count, alignment_log2)
				.or(self.second.allocate_contiguous_aligned(count, alignment_log2))
	}
}
