//! Provides pointer wrappers for safely accessing userspace.
//!
//! # User pointers
//!
//! TODO(doc).
//!
//! ## Local pointers
//!
//! TODO(doc).
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

use core::fmt;

mod impls {
	#[cfg(feature = "full")]
	cfg_select! {
		any(target_arch = "x86", target_arch = "x86_64") => {
			mod x86_64;
			pub use x86_64::*;
		}
	}
}

/// The error returned when a memory access to userspace failed.
#[expect(missing_copy_implementations, reason = "no future guarantees about being copy")]
#[derive(Debug)]
pub struct PointerError { _private: () }

impl PointerError {
	#[cfg(feature = "full")]
	const fn new() -> Self { Self { _private: () } }
}

impl fmt::Display for PointerError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "failed to write to pointer")
	}
}

impl core::error::Error for PointerError {}
