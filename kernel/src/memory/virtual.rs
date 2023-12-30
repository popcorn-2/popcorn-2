use kernel_api::memory::{AllocError, Page};
use kernel_api::memory::r#virtual::VirtualAllocator;

pub struct Global;

impl VirtualAllocator for Global {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, kernel_api::memory::AllocError> {
		todo!()
	}
	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		todo!()
	}
}
