use kernel_api::memory::Page;
use kernel_api::memory::r#virtual::VirtualAllocator;

pub struct Global;

impl VirtualAllocator for Global {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, kernel_api::memory::AllocError> {
		todo!()
	}
}
