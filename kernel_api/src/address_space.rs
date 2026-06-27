//! Types to manipulate the kernel and userspace address spaces.
//!
//! This module provides two sets of address space types - the [`Kernel`] and [`Userspace`] marker types
//! for use with [`Mapping`], as well as the [`AddressSpace`] type for directly
//! manipulating the address space of a thread.
//!
//! # Kernel address space
//!
//! TODO(doc).
//!
//! # User address space
//!
//! TODO(doc).

use alloc::borrow::Cow;
use alloc::sync::{Arc, Weak as WeakArc};
use core::any::Any;
use core::fmt::{Debug, Formatter};
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::ptr::DynMetadata;
use core::sync::atomic::{AtomicU128, Ordering};
use crate::allocator::{AllocError, Vmm};
use crate::mapping::{Caching, Mappable, Mapping, Protection};
use crate::memory::{RawFrame, RawPage};
use crate::sync::RwReadGuard;

#[cfg(debug_assertions)] use core::sync::atomic::AtomicBool;

#[doc(hidden)]
pub trait __AddressSpaceInner: Any + Debug + Send + Sync {}

#[derive(Debug)]
pub(crate) struct Weak(WeakArc<dyn __AddressSpaceInner>);

/// A userspace address space.
///
/// This owns a set of page tables, a virtual memory allocator, and a list of memory [`Mapping`]s.
#[deprecated = "renamed to `User`"]
pub type AddressSpace = User;

/// A userspace address space.
///
/// This owns a set of page tables, a virtual memory allocator, and a list of memory [`Mapping`]s.
///
/// See the [module level documentation](`self#user-address-space`) for more information.
// INVARIANT: `ptr` must always be a valid, transmuted pointer to `Arc::<dyn __AddressSpaceInner>::into_raw`
pub struct User {
	ptr: AtomicU128,
	#[cfg(debug_assertions)] personality: AtomicBool,
}

impl User {
	#[doc(hidden)]
	pub fn __new(address_space: Arc<dyn __AddressSpaceInner>) -> Self {
		let ptr = Arc::into_raw(address_space);
		Self {
			ptr: AtomicU128::new(Self::convert_u128(ptr)),
			#[cfg(debug_assertions)] personality: AtomicBool::new(false),
		}
	}

	pub(crate) fn downgrade(&self) -> Weak {
		let address_space = ManuallyDrop::new(self.clone());
		let ptr = address_space.__extract_ptr(Ordering::SeqCst);
		// SAFETY: `ptr` always comes from a call to `Arc::into_raw`
		let strong = unsafe { Arc::from_raw(ptr) };
		Weak(Arc::downgrade(&strong))
	}

	fn convert_u128(ptr: *const dyn __AddressSpaceInner) -> u128 {
		let (ptr, meta) = ptr.to_raw_parts();
		// SAFETY: `DynMetadata` is the address of a vtable so can be converted to a `usize`
		(ptr.expose_provenance() as u128) << 64 | unsafe { core::mem::transmute::<DynMetadata<dyn __AddressSpaceInner>, usize>(meta) } as u128
	}

	#[doc(hidden)]
	pub fn __extract_ptr(&self, ordering: Ordering) -> *const dyn __AddressSpaceInner {
		let val = self.ptr.load(ordering);
		// SAFETY: invariant of `AddressSpace` that `val` is a valid `dyn __AddressSpaceInner`
		let meta = unsafe { core::mem::transmute::<usize, DynMetadata<dyn __AddressSpaceInner>>(val.truncate::<usize>()) };
		let ptr = ptr::with_exposed_provenance::<()>((val >> 64).truncate::<usize>());
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

	pub(crate) fn push_mapping(&self, name: Cow<'static, str>, mapping: Mapping<Box<dyn Mappable + Send>, Self>) -> (MappingKey, impl DerefMut<Target = Mapping<Box<dyn Mappable + Send>, Self>>) {
		crate::bridge::address_space::user::push_mapping(self, name, mapping)
	}

	/// Returns `true` if the two `Arc`s point to the same allocation in a vein similar to
	/// [`ptr::eq`]. This function ignores the metadata of  `dyn Trait` pointers.
	pub fn ptr_eq(this: &Self, other: &Self) -> bool {
		this.ptr.load(Ordering::Relaxed) == other.ptr.load(Ordering::Relaxed)
	}

	/// Atomically replace the address space pointed to by `self`.
	///
	/// `swap` takes an [`Ordering`] argument which describes the memory ordering
	/// of this operation. All ordering modes are possible. Note that using
	/// [`Acquire`](`Ordering::Acquire`) makes the store part of this operation [`Relaxed`](`Ordering::Relaxed`), and
	/// using [`Release`](`Ordering::Release`) makes the load part [`Relaxed`](`Ordering::Relaxed`).
	///
	/// # Safety
	///
	/// Must not execute simultaneously with any other methods being called on `self`.
	#[must_use]
	pub unsafe fn swap(&self, other: Self, ordering: Ordering) -> Self {
		let load_ordering = match ordering {
			Ordering::Release | Ordering::AcqRel => Ordering::Relaxed,
			ordering => ordering,
		};

		let _guard = self.__assert_in_use();
		let other = ManuallyDrop::new(other);
		let other = other.ptr.load(load_ordering);
		let old = self.ptr.swap(other, ordering);
		Self {
			ptr: AtomicU128::new(old),
			#[cfg(debug_assertions)] personality: AtomicBool::new(false),
		}
	}
}

impl Clone for User {
	fn clone(&self) -> Self {
		let _guard = self.__assert_in_use();
		let ptr = self.__extract_ptr(Ordering::SeqCst);
		// SAFETY: `ptr` always comes from calling `Arc::into_raw`, and strong count
		// is only modified by `drop` and `clone`
		unsafe { Arc::increment_strong_count(ptr) };
		Self {
			ptr: AtomicU128::new(Self::convert_u128(ptr)),
			#[cfg(debug_assertions)] personality: AtomicBool::new(false),
		}
	}
}

impl Drop for User {
	fn drop(&mut self) {
		let _guard = self.__assert_in_use();
		let ptr = self.__extract_ptr(Ordering::Relaxed);
		// SAFETY: `ptr` always comes from calling `Arc::into_raw`, and strong count
		// is only modified by `drop` and `clone`
		unsafe { Arc::decrement_strong_count(ptr) }
	}
}

impl Debug for User {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let _guard = self.__assert_in_use();
		let ptr = self.__extract_ptr(Ordering::Relaxed);
		// SAFETY: result of `__extract_ptr` always points to a valid `dyn __AddressSpaceInner`
		unsafe { &*ptr }.fmt(f)
	}
}

/// A trait implemented by address spaces.
///
/// This trait is sealed: it cannot be implemented outside of this crate.
pub trait Ty: crate::sealed::Sealed {
	/// Returns the virtual memory allocator used for this address space.
	fn allocator(&self) -> impl Deref<Target = dyn Vmm> + '_;

	/// Maps `count` pages starting at `base_page` to `count` frames starting at `base_frame`.
	/// The protection and caching attributes specified are used.
	/// If mapping type metadata is enabled, then the [`Ty`](`crate::mapping::Ty`) passed will be
	/// stored.
	///
	/// # Errors
	///
	/// Returns a [`MapPageError`] if memory allocation or page table modification failed.
	fn map_contiguous(&self, base_page: RawPage, base_frame: RawFrame, count: usize, reason: crate::mapping::Ty, protection: Protection, caching: Caching) -> Result<(), MapPageError>;
}

impl Weak {
	pub fn ptr_eq(this: &Self, other: &User) -> bool {
		let val = this.0.as_ptr();
		User::convert_u128(val) == other.ptr.load(Ordering::Relaxed)
	}
}

/// The kernel address space.
///
/// See the [module level documentation](`self#kernel-address-space`) for more information.
#[derive(Debug)]
#[expect(clippy::field_scoped_visibility_modifiers, reason = "ZST which is only constructible by this crate")]
#[expect(missing_copy_implementations, reason = "no future guarantees about being copy")]
pub struct Kernel(pub(crate) ());

impl crate::sealed::Sealed for Kernel {}
impl crate::sealed::Sealed for User {}

const fn create_flags(protection: Protection, caching: Caching) -> u8 {
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
}

impl Ty for Kernel {
	fn allocator(&self) -> impl Deref<Target = dyn Vmm> + '_ {
		RwReadGuard::map(
			crate::bridge::memory::GLOBAL_VIRTUAL_ALLOCATOR.read(),
			|vmm| *vmm as &dyn Vmm
		)
	}

	fn map_contiguous(&self, base_page: RawPage, base_frame: RawFrame, count: usize, reason: crate::mapping::Ty, protection: Protection, caching: Caching) -> Result<(), MapPageError> {
		crate::bridge::address_space::kernel::map_contiguous(
			base_page,
			base_frame,
			count,
			reason,
			create_flags(protection, caching),
		)
	}
}

impl Ty for User {
	fn allocator(&self) -> impl Deref<Target = dyn Vmm> + '_ {
		crate::bridge::address_space::user::get_allocator(self)
    }

	fn map_contiguous(&self, base_page: RawPage, base_frame: RawFrame, count: usize, reason: crate::mapping::Ty, protection: Protection, caching: Caching) -> Result<(), MapPageError> {
		crate::bridge::address_space::user::map_contiguous(
			self,
			base_page,
			base_frame,
			count,
			reason,
			create_flags(protection, caching),
		)
	}
}

/// A key used to access a specific userspace mapping.
#[derive(Debug, Copy, Clone)]
pub struct MappingKey(#[doc(hidden)] pub usize);

/// The error type returned when modifying page table mappings.
#[derive(Debug, Copy, Clone)]
pub enum MapPageError {
	/// The requested virtual address is already mapped to another region.
	///
	/// If mapping type metadata is enabled, this will contain the type of mapping that
	/// already exists. See the [mapping type documentation](`crate::mapping::Ty`) for
	/// more information.
	AlreadyMapped(crate::mapping::Ty),
	/// An allocator returned an error when allocating memory needed for page tables.
	AllocError(AllocError),
}

impl From<AllocError> for MapPageError {
	fn from(value: AllocError) -> Self {
		Self::AllocError(value)
	}
}
