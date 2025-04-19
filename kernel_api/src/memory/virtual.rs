#![unstable(feature = "kernel_virtual_memory", issue = "none")]

use alloc::sync::{Arc, Weak};
use core::fmt::{Debug, Formatter};
use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::ops::DerefMut;
use core::ptr;
use core::ptr::NonNull;
use auto_impl::auto_impl;
use log::debug;
use crate::bridge::paging::{AddressSpaceInner, MapPageError};
use crate::memory::{Frame, Page};
use crate::sync::RwSpinlock;
use super::AllocError;

#[auto_impl(&, Box, Arc)]
pub trait VirtualAllocator: Send + Sync {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError>;
	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError>;
	fn deallocate_contiguous(&self, base: Page, len: usize);
}

extern "Rust" {
	#[link_name = "__popcorn_memory_virtual_kernel_global"]
	static GLOBAL_VIRTUAL_ALLOCATOR: RwSpinlock<&'static dyn VirtualAllocator>;
}

mod private {
	pub trait Sealed {}

	impl Sealed for super::Kernel {}
	impl Sealed for super::Userspace {}
}

pub struct Kernel;
pub struct Userspace(pub(super) AddressSpace);

#[unstable(feature = "kernel_mmap_config", issue = "24")]
pub trait AddressSpaceTy: private::Sealed {
	type PageTable<'a>;
	fn get_page_table(&self) -> Self::PageTable<'_>;
	fn translate_page(table: &mut Self::PageTable<'_>, page: Page) -> Option<Frame>;
	fn map_page(table: &mut Self::PageTable<'_>, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError>;
	fn unmap_page(table: &mut Self::PageTable<'_>, page: Page) -> Result<(), ()>;

	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError>;
	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError>;
	fn deallocate_contiguous(&self, base: Page, len: usize);
}

#[unstable(feature = "kernel_mmap_config", issue = "24")]
impl AddressSpaceTy for Kernel {
	type PageTable<'a> = impl DerefMut<Target = crate::bridge::paging::KTable>;
	
	fn get_page_table(&self) -> Self::PageTable<'_> {
		unsafe { crate::bridge::paging::__popcorn_paging_get_ktable() }
	}

	#[define_opaque()]
	fn translate_page(table: &mut Self::PageTable<'_>, page: Page) -> Option<Frame> {
		unsafe { crate::bridge::paging::__popcorn_paging_ktable_translate_page(&mut **table, page) }
	}

	#[define_opaque()]
	fn map_page(table: &mut Self::PageTable<'_>, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError> {
		unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut **table, page, frame, reason) }
	}

	#[define_opaque()]
	fn unmap_page(table: &mut Self::PageTable<'_>, page: Page) -> Result<(), ()> {
		unsafe { crate::bridge::paging::__popcorn_paging_ktable_unmap_page(&mut **table, page) }
	}

	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		let at = unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.read().allocate_contiguous(len)?;
		debug!("Global VMA allocated at {at:x?}+{len}");
		Ok(at)
	}

	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		debug!("Global VMA allocate contiguous {at:x?}+{len}");
		unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.read().allocate_contiguous_at(at, len)
	}
	
	fn deallocate_contiguous(&self, base: Page, len: usize) {
		unsafe { &GLOBAL_VIRTUAL_ALLOCATOR }.read().deallocate_contiguous(base, len)
	}
}

#[unstable(feature = "kernel_mmap_config", issue = "24")]
impl AddressSpaceTy for Userspace {
	type PageTable<'a> = &'a AddressSpaceInner;

	fn get_page_table(&self) -> Self::PageTable<'_> {
		self.0.as_ref()
	}

	fn translate_page(table: &mut Self::PageTable<'_>, page: Page) -> Option<Frame> {
		unsafe { crate::bridge::paging::__popcorn_paging_ttable_translate_page(*table, page) }
	}

	fn map_page(table: &mut Self::PageTable<'_>, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError> {
		unsafe { crate::bridge::paging::__popcorn_paging_ttable_map_page(*table, page, frame, reason) }
	}

	fn unmap_page(table: &mut Self::PageTable<'_>, page: Page) -> Result<(), ()> {
		unsafe { crate::bridge::paging::__popcorn_paging_ttable_unmap_page(*table, page) }
	}

	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		unsafe { crate::bridge::memory::__popcorn_address_space_allocate(self.0.as_ref(), len) }
	}

	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		unsafe { crate::bridge::memory::__popcorn_address_space_allocate_at(self.0.as_ref(), at, len) }
	}

	fn deallocate_contiguous(&self, base: Page, len: usize) {
		unsafe { crate::bridge::memory::__popcorn_address_space_deallocate(self.0.as_ref(), base, len) }
	}
}

pub struct OwnedPages<A: AddressSpaceTy> {
	base: Page,
	len: NonZero<usize>,
	address_space: A,
}

impl OwnedPages<Kernel> {
	pub fn new(len: NonZero<usize>) -> Result<Self, AllocError> {
		let base = Kernel.allocate_contiguous(len.get())?;
		Ok(Self {
			base,
			len,
			address_space: Kernel {}
		})
	}
}

impl OwnedPages<Userspace> {
	pub fn new_in(len: NonZero<usize>, address_space: &AddressSpace) -> Result<Self, AllocError> {
		let address_space = Userspace(AddressSpace::clone_ref(address_space));
		let base = address_space.allocate_contiguous(len.get())?;
		Ok(Self {
			base,
			len,
			address_space,
		})
	}
}

impl<A: AddressSpaceTy> OwnedPages<A> {
	pub fn into_raw_parts(self) -> (Page, NonZero<usize>, A) {
		let this = ManuallyDrop::new(self);
		(
			this.base,
			this.len,
			unsafe { ptr::read(&this.address_space) }
		)
	}

	pub unsafe fn from_raw_parts(base: Page, len: NonZero<usize>, address_space: A) -> Self {
		Self {
			base, len, address_space
		}
	}
}

impl<A: AddressSpaceTy> Drop for OwnedPages<A> {
	fn drop(&mut self) {
		self.address_space.deallocate_contiguous(self.base, self.len.get());
	}
}

pub struct AddressSpace(#[unstable(feature = "kernel_internals", issue = "none")] pub Arc<AddressSpaceInner>);
pub struct WeakAddressSpace(Weak<AddressSpaceInner>);

impl AddressSpace {
	pub fn as_ptr(&self) -> NonNull<AddressSpaceInner> {
		NonNull::from(Self::as_ref(self))
	}

	pub fn as_ref(&self) -> &AddressSpaceInner {
		Arc::as_ref(&self.0)
	}

	/// # Safety
	/// 
	/// The AddressSpace must stay alive until a new AddressSpace gets loaded
	pub unsafe fn load(&self) {
		unsafe { crate::bridge::paging::__popcorn_address_space_load(&self.0); }
	}

	pub fn clone_ref(this: &Self) -> AddressSpace {
		AddressSpace(Arc::clone(&this.0))
	}

	pub fn downgrade(this: &Self) -> WeakAddressSpace {
		WeakAddressSpace(Arc::downgrade(&this.0))
	}
}

impl Debug for AddressSpace {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("AddressSpace")
				.finish_non_exhaustive()
	}
}

impl WeakAddressSpace {
	pub fn upgrade(&self) -> Option<AddressSpace> {
		self.0.upgrade().map(AddressSpace)
	}
}
