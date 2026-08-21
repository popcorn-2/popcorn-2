//! Types to manipulate the kernel and userspace address spaces
//!
//! This module provides two sets of address space types - the [`Kernel`] and [`Userspace`] marker types
//! for use with (`Mapping`)[crate::mapping::Mapping], as well as the [`AddressSpace`] type for directly
//! manipulating the address space of a thread.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::sync::{Arc, Weak};
use core::any::Any;
use core::fmt::{Debug, Formatter};
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::ptr::DynMetadata;
use core::sync::atomic::Ordering;
use crate::allocator::Vmm;
use crate::mapping::{Caching, MapPageError, Mappable, Mapping, Protection};
use crate::memory::{RawFrame, RawPage};
use crate::sync::RwReadGuard;

#[cfg(debug_assertions)] use core::sync::atomic::AtomicBool;
use crate::num::{ufat, AtomicUfat};

// this an Arc around `kernel::memory::virtual::AddressSpaceInner`
// we could also use an extern type here instead of `dyn Any` but that
// means we need extra effort to keep uses of `drop(AddressSpace)`
// from panicking, as the last drop will panic trying to compute
// the offset of the data with `align_of_val`
//pub type AddressSpace = Arc<dyn Any + Send + Sync>;
#[derive(Debug)]
pub(crate) struct WeakAddressSpace(Weak<dyn Any + Send + Sync>);

pub struct AddressSpace {
	#[doc(hidden)]
	pub __ptr: AtomicUfat,
	#[cfg(debug_assertions)] personality: AtomicBool,
}

impl AddressSpace {
	#[doc(hidden)]
	pub fn __new(address_space: Arc<dyn Any + Send + Sync>) -> Self {
		let ptr = Arc::into_raw(address_space);
		AddressSpace {
			__ptr: AtomicUfat::new(Self::convert_ufat(ptr)),
			#[cfg(debug_assertions)] personality: AtomicBool::new(false),
		}
	}

	pub(crate) fn downgrade(&self) -> WeakAddressSpace {
		let address_space = ManuallyDrop::new(self.clone());
		let ptr = address_space.__extract_ptr(Ordering::SeqCst);
		let strong = unsafe { Arc::from_raw(ptr) };
		WeakAddressSpace(Arc::downgrade(&strong))
	}

	fn convert_ufat(ptr: *const (dyn Any + Send + Sync)) -> ufat {
		let (ptr, meta) = ptr.to_raw_parts();
		ufat::new(
			ptr.expose_provenance(),
			unsafe { core::mem::transmute::<DynMetadata<dyn Any + Send + Sync>, usize>(meta) }
		)
	}

	#[doc(hidden)]
	pub fn __extract_ptr(&self, ordering: Ordering) -> *const (dyn Any + Send + Sync) {
		let val = self.__ptr.load(ordering);
		let meta = unsafe { core::mem::transmute::<usize, DynMetadata<dyn Any + Send + Sync>>(val.lower()) };
		let ptr = ptr::with_exposed_provenance::<()>(val.upper());
		ptr::from_raw_parts(ptr, meta)
	}

	#[doc(hidden)]
	#[must_use]
	pub fn __assert_in_use(&self) -> impl Drop + '_ {
		#[cfg(debug_assertions)] {
			struct InUseGuard<'s>(&'s AtomicBool);

			impl Drop for InUseGuard<'_> {
				fn drop(&mut self) {
					self.0.store(false, Ordering::Release);
				}
			}

			self.personality.compare_exchange(
				false,
				true,
				Ordering::Acquire,
				Ordering::Relaxed
			).unwrap_or_else(|_| panic!("UB detected: potential race between `AddressSpace::clone()` and `AddressSpace::swap()`"));

			InUseGuard(&self.personality)
		}

		#[cfg(not(debug_assertions))] {
			struct S;

			impl Drop for S {
				fn drop(&mut self) {}
			}

			S
		}
	}

	pub(crate) fn push_mapping(&self, name: Cow<'static, str>, mapping: Mapping<Box<dyn Mappable + Send>, Userspace>) -> (MappingKey, impl DerefMut<Target = Mapping<Box<dyn Mappable + Send>, Userspace>>) {
		crate::bridge::address_space::user::push_mapping(self, name, mapping)
	}

	pub fn ptr_eq(this: &Self, other: &Self) -> bool {
		this.__ptr.load(Ordering::Relaxed) == other.__ptr.load(Ordering::Relaxed)
	}

	/// # Safety
	///
	/// Must not execute simultaneously with any other methods being called on `self`.
	pub unsafe fn swap(&self, other: AddressSpace, ordering: Ordering) -> AddressSpace {
		let _guard = self.__assert_in_use();
		let other = ManuallyDrop::new(other);
		let other = other.__ptr.load(ordering);
		let old = self.__ptr.swap(other, ordering);
		AddressSpace {
			__ptr: AtomicUfat::new(old),
			#[cfg(debug_assertions)] personality: AtomicBool::new(false),
		}
	}
}

impl Clone for AddressSpace {
	fn clone(&self) -> Self {
		let _guard = self.__assert_in_use();
		let ptr = self.__extract_ptr(Ordering::SeqCst);
		unsafe { Arc::increment_strong_count(ptr) };
		Self {
			__ptr: AtomicUfat::new(Self::convert_ufat(ptr)),
			#[cfg(debug_assertions)] personality: AtomicBool::new(false),
		}
	}
}

impl Drop for AddressSpace {
	fn drop(&mut self) {
		let _guard = self.__assert_in_use();
		let ptr = self.__extract_ptr(Ordering::Relaxed);
		unsafe { Arc::decrement_strong_count(ptr) }
	}
}

impl Debug for AddressSpace {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "AddressSpace {{ .. }}")
	}
}

pub trait Ty: crate::sealed::Sealed {
	fn allocator(&self) -> impl Deref<Target = dyn Vmm> + '_;
	fn map_contiguous(&self, base_page: RawPage, base_frame: RawFrame, count: usize, reason: crate::mapping::Ty, protection: Protection, caching: Caching) -> Result<(), MapPageError>;
}

impl WeakAddressSpace {
	pub fn ptr_eq(this: &Self, other: &AddressSpace) -> bool {
		let val = this.0.as_ptr();
		AddressSpace::convert_ufat(val) == other.__ptr.load(Ordering::Relaxed)
	}
}

#[non_exhaustive]
pub struct Kernel {}
pub struct Userspace {
	pub(crate) inner: AddressSpace,
}

impl crate::sealed::Sealed for Kernel {}
impl crate::sealed::Sealed for Userspace {}

impl Ty for Kernel {
	fn allocator(&self) -> impl Deref<Target = dyn Vmm> + '_ {
		RwReadGuard::map(
			crate::bridge::memory::GLOBAL_VIRTUAL_ALLOCATOR.read(),
			|vmm| *vmm as &dyn Vmm
		)
	}

	fn map_contiguous(&self, base_page: RawPage, base_frame: RawFrame, count: usize, reason: crate::mapping::Ty, protection: Protection, caching: Caching) -> Result<(), MapPageError> {
		let flags = {
			let mut base = 0u8;
			if protection.writable { base |= 1<<1; }
			if protection.executable { base |= 1<<2; }
			if protection.user_accessible { base |= 1<<3; }
			match caching {
				Caching::Normal => {},
				Caching::Mmio => { base |= 1<<5; },
				Caching::WriteCombine => { base |= 1<<4; },
			}
			base
		};

		crate::bridge::address_space::kernel::map_contiguous(
			base_page,
			base_frame,
			count,
			reason,
			flags,
		)
	}
}

impl Ty for Userspace {
	fn allocator(&self) -> impl Deref<Target = dyn Vmm> + '_ {
		crate::bridge::address_space::user::get_allocator(&self.inner)
    }

	fn map_contiguous(&self, base_page: RawPage, base_frame: RawFrame, count: usize, reason: crate::mapping::Ty, protection: Protection, caching: Caching) -> Result<(), MapPageError> {
		let flags = {
			let mut base = 0u8;
			if protection.writable { base |= 1<<1; }
			if protection.executable { base |= 1<<2; }
			if protection.user_accessible { base |= 1<<3; }
			match caching {
				Caching::Normal => {},
				Caching::Mmio => { base |= 1<<5; },
				Caching::WriteCombine => { base |= 1<<4; },
			}
			base
		};

		crate::bridge::address_space::user::map_contiguous(
			&self.inner,
			base_page,
			base_frame,
			count,
			reason,
			flags,
		)
	}
}

#[derive(Debug, Copy, Clone)]
pub struct MappingKey(pub usize);
