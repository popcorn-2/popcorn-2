#![unstable(feature = "kernel_internals", issue = "none")]

pub mod alloc {
	use core::alloc::Layout;

	extern "Rust" {
		pub fn __popcorn_alloc(layout: Layout) -> *mut u8;
		pub fn __popcorn_dealloc(ptr: *mut u8, layout: Layout);
		pub fn __popcorn_alloc_zeroed(layout: Layout) -> *mut u8;
		pub fn __popcorn_realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8;
	}
}

pub mod panic {
	use core::panic::PanicInfo;

	extern "Rust" {
		pub fn __popcorn_panic_handler(info: &PanicInfo) -> !;
		pub fn __popcorn_backtrace();
		pub fn __popcorn_is_panicking() -> bool;
	}
}

pub mod hal {
	extern "Rust" {
		pub fn __popcorn_enable_irq();
		pub fn __popcorn_disable_irq() -> bool;
	}
}

pub mod paging {
	use core::cell::RefMut;
	use core::ops::DerefMut;
	use crate::memory::{Frame, Page};
	use crate::memory::allocator::{AllocError, BackingAllocator, GlobalAllocator};

	extern "Rust" {
		pub type PageTable;

		pub fn __popcorn_paging_map_page(this: &mut PageTable, page: Page, frame: Frame, allocator: &dyn BackingAllocator) -> Result<(), MapPageError>;
	}

	pub unsafe fn __popcorn_paging_get_current_page_table() -> impl DerefMut<Target = PageTable> {
		extern "Rust" {
			pub fn __popcorn_paging_get_current_page_table() -> RefMut<'static, PageTable>;
		}

		__popcorn_paging_get_current_page_table()
	}

	#[derive(Debug, Copy, Clone)]
	pub enum MapPageError {
		AllocError,
		AlreadyMapped
	}

	#[doc(hidden)]
	impl From<AllocError> for MapPageError {
		fn from(_value: AllocError) -> Self {
			Self::AllocError
		}
	}
}

pub mod memory {
	use core::ops::Deref;
	use lock_api::MappedRwLockReadGuard;
	use crate::memory::allocator::GlobalAllocator;
	use crate::sync::rwlock::RwCount;

	pub unsafe fn __popcorn_memory_physical_get_kernel_highmem() -> impl Deref<Target = GlobalAllocator> {
		extern "Rust" {
			pub fn __popcorn_memory_physical_get_kernel_highmem() -> MappedRwLockReadGuard<'static, RwCount, GlobalAllocator>;
		}

		__popcorn_memory_physical_get_kernel_highmem()
	}

	extern "Rust" {
	}
}
