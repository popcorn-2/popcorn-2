use crate::memory::RawPage;
use crate::allocator::AllocError;

/// A virtual memory allocator.
pub trait Vmm {
	/// Allocates `count` number of contiguous pages.
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated. This
	/// does not mean there is no free memory, just that is there no region
	/// of contiguous memory large enough.
	#[must_use = "memory will be leaked unless the page is explicitly deallocated"]
	fn allocate_contiguous(&self, len: usize) -> Result<RawPage, AllocError>;

	/// Allocates `count` number of contiguous pages with the first frame being at `at`.
	///
	/// # Errors
	///
	/// Returns an [`AllocError`] if the memory could not be allocated. This
	/// does not mean there is no free memory, just that is there no region
	/// of contiguous memory large enough starting at `at`.
	#[must_use = "memory will be leaked unless the page is explicitly deallocated"]
	fn allocate_contiguous_at(&self, at: RawPage, len: usize) -> Result<RawPage, AllocError>;

	/// Deallocates `count` frames starting at `base`.
	// FIXME: why isn't this `unsafe` like the pmm equivalent is
	fn deallocate_contiguous(&self, base: RawPage, len: usize);
}

// todo: check why this exists
#[doc(hidden)]
impl Vmm for ! {
	fn allocate_contiguous(&self, _len: usize) -> Result<RawPage, AllocError> {
		*self
	}

	fn allocate_contiguous_at(&self, _at: RawPage, _len: usize) -> Result<RawPage, AllocError> {
		*self
	}

	fn deallocate_contiguous(&self, _base: RawPage, _len: usize) {}
}
