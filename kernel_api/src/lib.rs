//! This crate provides public facing types and interfaces used within the popcorn2 kernel

#![no_std]
#![feature(staged_api)]
#![feature(min_specialization)]
#![feature(custom_test_frameworks)]
#![feature(type_changing_struct_update)]
#![feature(step_trait)]
#![feature(generic_const_items)]
#![feature(generic_const_exprs)]
#![warn(missing_docs)]

#![stable(feature = "kernel_core_api", since = "0.1.0")]

extern crate alloc;

#[unstable(feature = "kernel_export_macro", issue = "none")]
pub use kernel_module_macros::module_export;

pub mod memory;
pub mod sync;
