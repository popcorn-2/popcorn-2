//! Provides kernel synchronisation primitives
//!
//! These are currently based on spinlocks but this may be changed in future

use core::ops::{Deref, DerefMut};
#[cfg(not(feature = "use_std"))]
pub use mutex::{Spinlock, SpinlockGuard, SpinlockGuardExt, MappedSpinlockGuard};
#[cfg(feature = "use_std")]
pub use parking_lot::{Mutex as Spinlock, MutexGuard as SpinlockGuard, MappedMutexGuard as MappedSpinlockGuard};
#[cfg(not(feature = "use_std"))]
pub use send_wrapper::*;

#[cfg(not(feature = "use_std"))]
pub use once::{LazyLock, Once, OnceLock, BootstrapOnceLock};
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

pub struct Syncify<T>(T);

impl<T> Syncify<T> {
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
