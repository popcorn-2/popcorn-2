//! The unstable interface between the kernel and kernel modules.
#![warn(clippy::missing_docs_in_private_items)]

/// ABI for kernel version 0.1.0.
#[cfg(kernel_version = "0.1.0")]
/// ABI for kernel version 0.1.0.
mod kernel_abi_v1;

#[cfg(kernel_version = "0.1.0")]
pub use kernel_abi_v1::*;
