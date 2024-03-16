//! Provides kernel synchronisation primitives
//!
//! These are currently based on spinlocks but this may be changed in future

#![stable(feature = "kernel_core_api", since = "0.1.0")]

use core::ops::Deref;
#[cfg(not(feature = "use_std"))]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use mutex::{Mutex, MutexGuard};
#[cfg(feature = "use_std")]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use parking_lot::{Mutex, MutexGuard};

#[cfg(not(feature = "use_std"))]
#[unstable(feature = "kernel_spinlocks", issue = "none")]
pub use mutex::{Spinlock, SpinlockGuard};

#[cfg(not(feature = "use_std"))]
#[unstable(feature = "kernel_sync_once", issue = "none")]
pub use once::{LazyLock, Once, OnceLock, BootstrapOnceLock};
#[cfg(feature = "use_std")]
#[unstable(feature = "kernel_sync_once", issue = "none")]
pub use std::sync::{LazyLock, Once, OnceLock};

#[cfg(not(feature = "use_std"))]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use rwlock::{RwLock, RwReadGuard, RwUpgradableReadGuard, RwWriteGuard};
#[cfg(feature = "use_std")]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use parking_lot::{RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard};

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
unsafe impl<T> Sync for Syncify<T> {}
