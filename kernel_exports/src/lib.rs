#![feature(allocator_api)]
#![feature(error_in_core)]
#![no_std]

pub use kernel_module_macros::*;

mod macros;
mod bridge;
