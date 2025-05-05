#![unstable(feature = "kernel_virtual_memory", issue = "none")]

use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::ops::DerefMut;
use core::ptr;
use auto_impl::auto_impl;
use log::debug;
use crate::bridge::paging::MapPageError;
use crate::memory::{Frame, Page};
use crate::memory::mapping::{Location, Protection};
use crate::memory::r#virtual::address_space::{AddressSpace, AddressSpaceInner, Weak};
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
pub struct Userspace(pub(super) Weak);

#[unstable(feature = "kernel_mmap_config", issue = "24")]
pub trait AddressSpaceTy: private::Sealed {
	type PageTable<'a>;
	fn get_page_table(&self) -> Self::PageTable<'_>;
	fn translate_page(table: &mut Self::PageTable<'_>, page: Page) -> Option<Frame>;
	fn map_page(table: &mut Self::PageTable<'_>, page: Page, frame: Frame, reason: u16, protection: Protection) -> Result<(), MapPageError>;
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
	fn map_page(table: &mut Self::PageTable<'_>, page: Page, frame: Frame, reason: u16, protection: Protection) -> Result<(), MapPageError> {
		unsafe { crate::bridge::paging::__popcorn_paging_ktable_map_page(&mut **table, page, frame, reason, protection) }
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
	type PageTable<'a> = Option<AddressSpace>;

	fn get_page_table(&self) -> Self::PageTable<'_> {
		Weak::upgrade(&self.0)
	}

	fn translate_page(table: &mut Self::PageTable<'_>, page: Page) -> Option<Frame> {
		let Some(table) = table else { return None };
		unsafe { crate::bridge::paging::__popcorn_paging_ttable_translate_page(table.as_ref(), page) }
	}

	fn map_page(table: &mut Self::PageTable<'_>, page: Page, frame: Frame, reason: u16, protection: Protection) -> Result<(), MapPageError> {
		let Some(table) = table else { return Err(MapPageError::AllocError) };
		unsafe { crate::bridge::paging::__popcorn_paging_ttable_map_page(table.as_ref(), page, frame, reason, protection) }
	}

	fn unmap_page(table: &mut Self::PageTable<'_>, page: Page) -> Result<(), ()> {
		let Some(table) = table else { return Ok(()) };
		unsafe { crate::bridge::paging::__popcorn_paging_ttable_unmap_page(table.as_ref(), page) }
	}

	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		let address_space = Weak::upgrade(&self.0).ok_or(AllocError)?;
		let at = unsafe { crate::bridge::memory::__popcorn_address_space_allocate(address_space.as_ref(), len)? };
		debug!("Userspace VMA allocated at {at:x?}+{len}");
		Ok(at)
	}

	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		let address_space = Weak::upgrade(&self.0).ok_or(AllocError)?;
		debug!("Userapce VMA allocate contiguous {at:x?}+{len}");
		unsafe { crate::bridge::memory::__popcorn_address_space_allocate_at(address_space.as_ref(), at, len) }
	}

	fn deallocate_contiguous(&self, base: Page, len: usize) {
		let Some(address_space) = Weak::upgrade(&self.0) else { return; };
		unsafe { crate::bridge::memory::__popcorn_address_space_deallocate(address_space.as_ref(), base, len) }
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
		let address_space = Userspace(AddressSpace::downgrade(address_space));
		let base = address_space.allocate_contiguous(len.get())?;
		Ok(Self {
			base,
			len,
			address_space,
		})
	}
	
	pub fn xnew(count: NonZero<usize>, address_space: &AddressSpace, location: Location<Page>) -> Result<Self, AllocError> {
		match location {
			Location::Any => Self::new_in(count, address_space),
			Location::At(f) => {
				let address_space = Userspace(AddressSpace::downgrade(address_space));
				let base = address_space.allocate_contiguous_at(f, count.get())?;
				Ok(OwnedPages {
					base,
					len: count,
					address_space
				})
			},
			_ => todo!(),
		}
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

pub mod address_space {
	use alloc::sync;
	use alloc::sync::Arc;
	use core::fmt::{Debug, Formatter};
	use core::marker::PhantomData;
	use core::ptr::NonNull;

	extern "Rust" {
		type _AddressSpaceInner;
	}

	#[repr(align(8))]
	pub struct AddressSpaceInner((), PhantomData<_AddressSpaceInner>);

	#[repr(transparent)]
	pub struct AddressSpace(#[unstable(feature = "kernel_internals", issue = "none")] pub Arc<AddressSpaceInner>);
	pub struct Weak(sync::Weak<AddressSpaceInner>);

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
		
		pub fn downgrade(this: &Self) -> Weak {
			Weak(Arc::downgrade(&this.0))
		}
	}
	
	impl Weak {
		pub fn upgrade(this: &Self) -> Option<AddressSpace> {
			sync::Weak::upgrade(&this.0).map(AddressSpace)
		}
	}

	impl Debug for AddressSpace {
		fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
			f.debug_tuple("AddressSpace")
			 .finish_non_exhaustive()
		}
	}
}
