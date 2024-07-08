//! This crate provides public facing types and interfaces used within the popcorn2 kernel

#![cfg_attr(not(feature = "use_std"), no_std)]
#![feature(staged_api)]
#![feature(min_specialization)]
#![feature(custom_test_frameworks)]
#![feature(type_changing_struct_update)]
#![feature(step_trait)]
#![feature(generic_const_items)]
#![feature(generic_const_exprs)]
#![feature(type_alias_impl_trait)]
#![feature(extern_types)]
#![feature(new_uninit)]
#![feature(doc_auto_cfg)]
#![feature(doc_cfg_hide)]
#![feature(let_chains)]
#![cfg_attr(feature = "use_std", feature(lazy_cell))]
#![warn(missing_docs)]

#![doc(issue_tracker_base_url = "https://github.com/popcorn-2/popcorn-2/issues/")]
#![doc(cfg_hide(all(not(feature = "use_std"), feature = "full")))]
#![doc(cfg_hide(feature = "full"))]
#![doc(cfg_hide(not(feature = "use_std")))]

#![stable(feature = "kernel_core_api", since = "0.1.0")]

extern crate alloc;

#[unstable(feature = "kernel_export_macro", issue = "none")]
pub use kernel_module_macros::module_export;

pub mod memory;
#[cfg(feature = "full")]
pub mod sync;

#[cfg(all(not(feature = "use_std"), feature = "full"))]
pub mod bridge;

pub mod ptr;

#[cfg(all(not(feature = "use_std"), feature = "full"))]
pub mod time;
