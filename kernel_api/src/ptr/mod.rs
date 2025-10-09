//! Provides pointer wrappers for safely accessing userspace
//! 
//! All pointers provided from userspace should be accessed through a [`User`] to
//! prevent kernelspace page faults from invalid or misaligned pointers.
//! 
//! # SMAP
//! 
//! When SMAP is supported on the system, all memory access through a [`User`] will
//! automatically set and clear the `AC` flag to prevent trapping.

use core::fmt;
use core::marker::PhantomData;
use core::ptr::NonNull;

#[cfg(feature = "full")]
mod user_ptr;
#[cfg(feature = "full")]
pub use user_ptr::*;

#[cfg(feature = "full")]
mod user_local;
#[cfg(feature = "full")]
pub use user_local::*;

#[doc(hidden)]
pub struct Unique<T: ?Sized> {
	pointer: NonNull<T>,
	_marker: PhantomData<T>,
}

/// `Unique` pointers are `Send` if `T` is `Send` because the data they
/// reference is unaliased. Note that this aliasing invariant is
/// unenforced by the type system; the abstraction using the
/// `Unique` must enforce it.
unsafe impl<T: Send + ?Sized> Send for Unique<T> { }

/// `Unique` pointers are `Sync` if `T` is `Sync` because the data they
/// reference is unaliased. Note that this aliasing invariant is
/// unenforced by the type system; the abstraction using the
/// `Unique` must enforce it.
unsafe impl<T: Sync + ?Sized> Sync for Unique<T> { }

impl<T: Sized> Unique<T> {
	/// Creates a new `Unique` that is dangling, but well-aligned.
	///
	/// This is useful for initializing types which lazily allocate, like
	/// `Vec::new` does.
	pub fn empty() -> Self {
		unsafe {
			Unique::new(NonNull::dangling().as_ptr())
		}
	}
}

impl<T: ?Sized> Unique<T> {
	/// Creates a new `Unique`.
	///
	/// # Safety
	///
	/// `ptr` must be non-null.
	pub const unsafe fn new(ptr: *mut T) -> Unique<T> {
		Unique { pointer: NonNull::new_unchecked(ptr), _marker: PhantomData }
	}

	/// Acquires the underlying `*mut` pointer.
	pub fn as_ptr(self) -> *mut T {
		self.pointer.as_ptr()
	}

	/// Dereferences the content.
	///
	/// The resulting lifetime is bound to self so this behaves "as if"
	/// it were actually an instance of T that is getting borrowed. If a longer
	/// (unbound) lifetime is needed, use `&*my_ptr.ptr()`.
	pub unsafe fn as_ref(&self) -> &T {
		&*self.as_ptr()
	}

	/// Mutably dereferences the content.
	///
	/// The resulting lifetime is bound to self so this behaves "as if"
	/// it were actually an instance of T that is getting borrowed. If a longer
	/// (unbound) lifetime is needed, use `&mut *my_ptr.ptr()`.
	pub unsafe fn as_mut(&mut self) -> &mut T {
		&mut *self.as_ptr()
	}
}

impl<T: ?Sized> Clone for Unique<T> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<T: ?Sized> Copy for Unique<T> {}

impl<T: ?Sized> fmt::Pointer for Unique<T> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		fmt::Pointer::fmt(&self.as_ptr(), f)
	}
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[path = "x86_64.rs"]
#[cfg(feature = "full")]
mod impls;

/// The error returned when a memory access to userspace failed
#[derive(Debug)]
#[non_exhaustive]
pub struct PointerError {}
