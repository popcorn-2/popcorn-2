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
		pub fn __popcorn_set_irq(state: usize);
		pub fn __popcorn_disable_irq() -> usize;
	}
}

pub mod paging {
	use core::marker::PhantomData;
	use core::ops::DerefMut;
	use alloc::sync::Arc;
	use crate::memory::{Frame, Page, PhysicalAddress, VirtualAddress, AllocError};
	use crate::sync::RwWriteGuard;
	use crate::memory::allocator::PhysicalAllocator;

	// FIXME: replace with extern type when alignment can be specified
	#[repr(align(8))]
	pub struct KTable((), PhantomData<KTableInner>);

	#[repr(align(8))]
	pub struct AddressSpaceInner((), PhantomData<_AddressSpaceInner>);

	extern "Rust" {
		type KTableInner;
		type _AddressSpaceInner;

		pub fn __popcorn_paging_ktable_translate_page(this: &KTable, page: Page) -> Option<Frame>;
		pub fn __popcorn_paging_ktable_translate_address(this: &KTable, addr: VirtualAddress) -> Option<PhysicalAddress>;
		pub fn __popcorn_paging_ktable_map_page(this: &mut KTable, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError>;
		pub fn __popcorn_paging_ktable_unmap_page(this: &mut KTable, page: Page) -> Result<(), ()>;
		pub fn __popcorn_paging_ttable_translate_page(this: &AddressSpaceInner, page: Page) -> Option<Frame>;
		pub fn __popcorn_paging_ttable_translate_address(this: &AddressSpaceInner, addr: VirtualAddress) -> Option<PhysicalAddress>;
		pub fn __popcorn_paging_ttable_map_page(this: &AddressSpaceInner, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError>;
		pub fn __popcorn_paging_ttable_unmap_page(this: &AddressSpaceInner, page: Page) -> Result<(), ()>;
		pub fn __popcorn_address_space_load(this: &AddressSpaceInner);
	}

	pub unsafe fn __popcorn_paging_get_ktable() -> impl DerefMut<Target = KTable> {
		extern "Rust" {
			pub fn __popcorn_paging_get_ktable() -> RwWriteGuard<'static, KTable>;
		}

		__popcorn_paging_get_ktable()
	}

	#[derive(Debug, Copy, Clone)]
	pub enum MapPageError {
		AllocError,
		AlreadyMapped(u16)
	}

	#[doc(hidden)]
	impl From<AllocError> for MapPageError {
		fn from(_value: AllocError) -> Self {
			Self::AllocError
		}
	}
}

pub mod memory {
	use core::ptr::NonNull;
	use crate::memory::physical::GlobalAllocator;
	use crate::bridge::paging::AddressSpaceInner;
	use crate::memory::{AllocError, Page};

	extern "Rust" {
		#[link_name = "__popcorn_memory_physical_highmem"]
		pub static GLOBAL_HIGHMEM: GlobalAllocator;

		#[link_name = "__popcorn_memory_physical_dmamem"]
		pub static GLOBAL_DMA: GlobalAllocator;
		
		pub fn __popcorn_check_address_space(to_check: NonNull<AddressSpaceInner>) -> bool;
		pub fn __popcorn_address_space_allocate(this: &AddressSpaceInner, len: usize) -> Result<Page, AllocError>;
		pub fn __popcorn_address_space_allocate_at(this: &AddressSpaceInner, at: Page, len: usize) -> Result<Page, AllocError>;
		pub fn __popcorn_address_space_deallocate(this: &AddressSpaceInner, base: Page, len: usize);
	}
}

pub mod time {
	use core::num::NonZero;

	extern "Rust" {
		#[link_name = "__popcorn_system_time"]
		pub fn system_time() -> u128;

		#[link_name = "__popcorn_system_time_scale"]
		pub fn system_time_to_nanos() -> (u128, NonZero<u128>);
	}
}
