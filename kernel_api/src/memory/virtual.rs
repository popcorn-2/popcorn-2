#![unstable(feature = "kernel_virtual_memory", issue = "none")]

use crate::memory::Page;
use super::AllocError;

pub trait VirtualAllocator {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError>;
}
