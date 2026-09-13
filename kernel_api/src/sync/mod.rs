//! Provides kernel synchronisation primitives.
//!
//! # Overview
//!
//! The following is an overview of the available synchronisation objects:
//! - [`Spinlock`]: Provides mutual exclusion by [busy-waiting](https://en.wikipedia.org/wiki/Busy_waiting) until
//!   the protected resource is available.
//! - [`RwSpinlock`]: An alternative to a plain `Spinlock` which allows for multiple concurrent readers, which
//!   can be more efficient for read-heavy workloads.
//! - [`IrqCell`]: A single-threaded mutual exclusion primitive, similar to [`RefCell`](`core::cell::RefCell`),
//!   which attempts to reduce deadlocks by disabling interrupts while locked.
//! - [`OnceLock`]: Used for thread-safe, one-time initialization of a variable, with potentially different
//!   initializers based on the caller.
//! - [`LazyLock`]: Used for thread-safe, one-time initialization of a variable, using one nullary initializer
//!   function provided at creation.
//! - [`SendWrapper`]: A wrapper type around a non-[`Send`] object to allow moving the object between threads
//!   by tracking the original thread it was created in.

#[cfg(not(feature = "use_std"))]
pub use mutex::{Spinlock, SpinlockGuard, SpinlockGuardExt, MappedSpinlockGuard};
#[cfg(feature = "use_std")]
pub use parking_lot::{Mutex as Spinlock, MutexGuard as SpinlockGuard, MappedMutexGuard as MappedSpinlockGuard};
#[cfg(not(feature = "use_std"))]
pub use send_wrapper::*;

#[cfg(not(feature = "use_std"))]
pub use once::{LazyLock, Once, OnceLock};
#[cfg(feature = "use_std")]
pub use std::sync::{LazyLock, Once, OnceLock};

#[cfg(not(feature = "use_std"))]
pub use rwlock::{RwSpinlock, RwReadGuard, RwUpgradableReadGuard, RwWriteGuard};
#[cfg(feature = "use_std")]
pub use parking_lot::{RwLock as RwSpinlock, RwLockReadGuard as RwReadGuard, RwLockUpgradableReadGuard as RwUpgradableReadGuard, RwLockWriteGuard as RwWriteGuard};

#[cfg(not(feature = "use_std"))]
pub use irq_cell::{IrqCell, IrqGuard};

#[cfg(not(feature = "use_std"))]
mod mutex;

#[cfg(not(feature = "use_std"))]
pub(crate) mod rwlock;

#[cfg(not(feature = "use_std"))]
mod once;

#[cfg(not(feature = "use_std"))]
mod irq_cell;

#[cfg(not(feature = "use_std"))]
mod send_wrapper;

#[doc(hidden)]
pub struct Syncify<T>(T);

impl<T> Syncify<T> {
	/// # Safety
	///
	/// The contained value must only be accessed (including dropping) on other threads if it is [`Sync`]/[`Send`].
	pub unsafe fn new(t: T) -> Self { Self(t) }

	pub fn into_inner(self) -> T { self.0 }
}

impl<T> Deref for Syncify<T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<T> DerefMut for Syncify<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

unsafe impl<T> Sync for Syncify<T> {}
unsafe impl<T> Send for Syncify<T> {}
