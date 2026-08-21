//! Provides pointer wrappers for safely accessing userspace
//! 
//! All pointers provided from userspace should be accessed through a [`User`] to
//! prevent kernelspace page faults from invalid or misaligned pointers.
//! 
//! # SMAP
//! 
//! When SMAP is supported on the system, all memory access through a [`User`] will
//! automatically set and clear the `AC` flag to prevent trapping.

#[cfg(feature = "full")]
mod user_ptr;
#[cfg(feature = "full")]
pub use user_ptr::*;

#[cfg(feature = "full")]
mod user_local;
#[cfg(feature = "full")]
pub use user_local::*;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[path = "x86_64.rs"]
#[cfg(feature = "full")]
mod impls;

/// The error returned when a memory access to userspace failed
#[derive(Debug)]
#[non_exhaustive]
pub struct PointerError {}
