#![unstable(feature = "kernel_virtual_memory", issue = "none")]

use core::sync::atomic::{AtomicUsize, Ordering};
use crate::memory::{Page, VirtualAddress};
use super::AllocError;

pub trait VirtualAllocator {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError>;
	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError>;
	fn deallocate_contiguous(&self, base: Page, len: usize);
}

static FIXME_END: AtomicUsize = AtomicUsize::new(0x10000);

pub struct ThisNeedsFixing;

impl VirtualAllocator for ThisNeedsFixing {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		let old = match len {
			0 => FIXME_END.load(Ordering::Relaxed),
			1.. => FIXME_END.fetch_add(len * 4096, Ordering::Relaxed)
		};

		if old > 0x1_0000_0000_0000 { return Err(AllocError); }

		Ok(Page::new(VirtualAddress::new(old)))
	}

	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		let current_end = FIXME_END.load(Ordering::Relaxed);

		if current_end != at.start().addr { return Err(AllocError); }

		match FIXME_END.compare_exchange(current_end, current_end + len * 4098, Ordering::Relaxed, Ordering::Relaxed) {
			Ok(_) => Ok(at),
			Err(_) => Err(AllocError)
		}
	}

	fn deallocate_contiguous(&self, _: Page, _: usize) {}
}
