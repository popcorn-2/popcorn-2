use crate::memory::RawPage;
use crate::allocator::AllocError;

/// A virtual memory allocator
pub trait Vmm {
	fn allocate_contiguous(&self, len: usize) -> Result<RawPage, AllocError>;
	fn allocate_contiguous_at(&self, at: RawPage, len: usize) -> Result<RawPage, AllocError>;
	fn deallocate_contiguous(&self, base: RawPage, len: usize);
}

impl Vmm for ! {
	fn allocate_contiguous(&self, _len: usize) -> Result<RawPage, AllocError> {
		*self
	}

	fn allocate_contiguous_at(&self, _at: RawPage, _len: usize) -> Result<RawPage, AllocError> {
		*self
	}

	fn deallocate_contiguous(&self, _base: RawPage, _len: usize) {}
}
