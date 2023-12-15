//! Provides kernel synchronisation primitives
//!
//! These are currently based on spinlocks but this may be changed in future

#![stable(feature = "kernel_core_api", since = "0.1.0")]

#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use mutex::{Mutex, MutexGuard};
#[unstable(feature = "kernel_spinlocks", issue = "none")]
pub use mutex::{Spinlock, SpinlockGuard};
#[unstable(feature = "kernel_sync_once", issue = "none")]
pub use once::{LazyLock, Once, OnceLock};
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub use rwlock::{RwLock, RwReadGuard, RwUpgradableReadGuard, RwWriteGuard};

mod mutex;
mod rwlock;
mod once;

