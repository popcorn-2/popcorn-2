use core::sync::atomic::{AtomicUsize, Ordering};
use kernel_api::memory::{AllocError, Page, VirtualAddress};
use kernel_api::memory::r#virtual::VirtualAllocator;

#[export_name = "__popcorn_memory_virtual_kernel_global"]
static GLOBAL_VIRTUAL_ALLOCATOR: dyn* VirtualAllocator = &BOOTSTRAP;

static BOOTSTRAP: Bootstrap = Bootstrap { end: AtomicUsize::new(0x10000) };

pub struct Bootstrap {
	end: AtomicUsize
}

impl VirtualAllocator for Bootstrap {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		let old = match len {
			0 => self.end.load(Ordering::Relaxed),
			1.. => self.end.fetch_add(len * 4096, Ordering::Relaxed)
		};

		if old > 0x1_0000_0000_0000 { return Err(AllocError); }

		Ok(Page::new(VirtualAddress::new(old)))
	}

	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		let current_end = self.end.load(Ordering::Relaxed);

		if current_end != at.start().addr { return Err(AllocError); }

		match self.end.compare_exchange(current_end, current_end + len * 4098, Ordering::Relaxed, Ordering::Relaxed) {
			Ok(_) => Ok(at),
			Err(_) => Err(AllocError)
		}
	}

	fn deallocate_contiguous(&self, _: Page, _: usize) {}
}

pub use kernel_api::memory::r#virtual::Global;
