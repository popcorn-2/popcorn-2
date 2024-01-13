use core::ptr::addr_of;
use core::sync::atomic::{AtomicPtr, Ordering};
use log::debug;
use kernel_api::memory::{AllocError, Page, VirtualAddress};
use kernel_api::memory::r#virtual::VirtualAllocator;
use kernel_api::sync::RwLock;

#[export_name = "__popcorn_memory_virtual_kernel_global"]
pub static GLOBAL_VIRTUAL_ALLOCATOR: RwLock<&'static dyn VirtualAllocator> = RwLock::new(&BOOTSTRAP);

extern "C" {
	static __popcorn_vmem_bootstrap_start: u8;
	static __popcorn_vmem_bootstrap_end: u8;
}

pub struct SyncWrapper(pub *mut u8);
unsafe impl Sync for SyncWrapper {}

pub static VMEM_BOOTSTRAP_START: SyncWrapper = SyncWrapper(unsafe { addr_of!(__popcorn_vmem_bootstrap_start) as *mut _ });
pub static VMEM_BOOTSTRAP_END: SyncWrapper = SyncWrapper(unsafe { addr_of!(__popcorn_vmem_bootstrap_end) as *mut _ });

static BOOTSTRAP: Bootstrap = Bootstrap {
	start: AtomicPtr::new(VMEM_BOOTSTRAP_START.0)
};

pub struct Bootstrap {
	start: AtomicPtr<u8>
}

impl VirtualAllocator for Bootstrap {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		let old = match len {
			0 => self.start.load(Ordering::Relaxed), // this doesn't make sense/needs redesigning but was required to not mess up heap API too much
			1.. => self.start.fetch_byte_add(len * 4096, Ordering::Relaxed)
		};

		if old > VMEM_BOOTSTRAP_END.0 { return Err(AllocError); }

		Ok(Page::new(VirtualAddress::new(old as usize)))
	}

	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		let current_end = self.start.load(Ordering::Relaxed);

		debug!("Bootstrap VMA current_end={current_end:#p}");

		if current_end != at.start().as_ptr() {
			debug!("`at` end doesn't match");
			return Err(AllocError);
		}
		if current_end > VMEM_BOOTSTRAP_END.0 {
			debug!("exhausted vmem_bootstrap region");
			return Err(AllocError);
		}

		match self.start.compare_exchange(current_end, unsafe { current_end.byte_add(len * 4096) }, Ordering::Relaxed, Ordering::Relaxed) {
			Ok(_) => Ok(at),
			Err(_) => Err(AllocError)
		}
	}

	fn deallocate_contiguous(&self, _: Page, _: usize) {}
}

pub use kernel_api::memory::r#virtual::Global;
