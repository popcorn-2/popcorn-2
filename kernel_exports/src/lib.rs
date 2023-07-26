#![feature(allocator_api)]
#![feature(error_in_core)]
#![no_std]

use core::alloc::{GlobalAlloc, Layout};
pub use kernel_module_macros::*;

mod macros;
mod bridge;

pub mod memory;
pub mod sync;
