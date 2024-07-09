//! Provides kernel synchronisation primitives
//!
//! These are currently based on spinlocks but this may be changed in future

#![stable(feature = "kernel_core_api", since = "0.1.0")]

use core::ops::{Deref, DerefMut};
#[cfg(not(feature = "use_std"))]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use mutex::{Spinlock, SpinlockGuard, SpinlockGuardExt};
#[cfg(feature = "use_std")]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use parking_lot::{Mutex as Spinlock, MutexGuard as SpinlockGuard};

#[cfg(not(feature = "use_std"))]
#[unstable(feature = "kernel_sync_once", issue = "none")]
pub use once::{LazyLock, Once, OnceLock, BootstrapOnceLock};
#[cfg(feature = "use_std")]
#[unstable(feature = "kernel_sync_once", issue = "none")]
pub use std::sync::{LazyLock, Once, OnceLock};

#[cfg(not(feature = "use_std"))]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use rwlock::{RwSpinlock, RwReadGuard, RwUpgradableReadGuard, RwWriteGuard};
#[cfg(feature = "use_std")]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use parking_lot::{RwLock as RwSpinlock, RwLockReadGuard as RwReadGuard, RwLockUpgradableReadGuard as RwUpgradableReadGuard, RwLockWriteGuard as RwWriteGuard};

#[cfg(not(feature = "use_std"))]
mod mutex;

#[cfg(not(feature = "use_std"))]
pub(crate) mod rwlock;

#[cfg(not(feature = "use_std"))]
mod once;

#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub struct Syncify<T>(T);

impl<T> Syncify<T> {
	#[stable(feature = "kernel_core_api", since = "0.1.0")]
	pub unsafe fn new(t: T) -> Self { Self(t) }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<T> Deref for Syncify<T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
impl<T> DerefMut for Syncify<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
unsafe impl<T> Sync for Syncify<T> {}
#[stable(feature = "kernel_core_api", since = "0.1.0")]
unsafe impl<T> Send for Syncify<T> {}
