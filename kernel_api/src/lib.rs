//! Popcorn2 kernel public API
//! 
//! Provides a mix of utility types and functions to replace `std`, as well as a stable API to kernel internals.

#![feature(min_specialization)]
#![feature(step_trait)]
#![feature(doc_cfg)]
#![feature(arbitrary_self_types_pointers)]
#![feature(cfg_target_has_atomic)]
#![feature(const_trait_impl)]
#![feature(const_convert)]
#![feature(const_ops)]
#![feature(const_option_ops)]

#![cfg_attr(target_has_atomic_load_store = "128", feature(integer_atomics))]

#![cfg_attr(feature = "full", feature(type_changing_struct_update))]
#![cfg_attr(feature = "full", feature(never_type))]
#![cfg_attr(feature = "full", feature(debug_closure_helpers))]
#![cfg_attr(feature = "full", feature(unsize))]
#![cfg_attr(feature = "full", feature(dispatch_from_dyn))]
#![cfg_attr(feature = "full", feature(coerce_unsized))]
#![cfg_attr(feature = "full", feature(pointer_is_aligned_to))]
#![cfg_attr(feature = "full", feature(slice_ptr_get))]
#![cfg_attr(feature = "full", feature(ptr_metadata))]
#![cfg_attr(feature = "full", feature(maybe_uninit_fill))]
#![cfg_attr(feature = "full", feature(sanitize))]
#![cfg_attr(feature = "full", feature(negative_impls))]

//#![deny(warnings)]
#![deny(unfulfilled_lint_expectations)]
#![warn(missing_docs)]
//#![deny(rustdoc::broken_intra_doc_links)]
#![warn(rustdoc::private_intra_doc_links)]
//#![warn(rustdoc::missing_doc_code_examples)]
#![warn(rustdoc::invalid_codeblock_attributes)]
#![warn(rustdoc::invalid_html_tags)]
#![warn(rustdoc::invalid_rust_codeblocks)]
#![warn(rustdoc::bare_urls)]

#![allow(type_alias_bounds)]

#![cfg_attr(not(feature = "use_std"), no_std)]

extern crate alloc;

pub mod memory;

#[cfg(feature = "full")]
pub mod sync;

#[cfg(feature = "full")]
mod bridge;

pub mod ptr;

#[cfg(feature = "full")]
pub mod time;

#[cfg(feature = "full")]
pub mod detect;

#[cfg(feature = "full")]
pub mod address_space;

pub mod mapping;

#[cfg(feature = "full")]
pub mod allocator;

#[cfg(feature = "full")]
pub mod threading;

#[cfg(feature = "full")]
pub mod syscall;

#[cfg(feature = "full")]
pub mod executor;

#[cfg(feature = "full")]
pub mod channel;

mod sealed {
    pub trait Sealed {}
}

/// Prints and returns the value of a given expression for quick and dirty debugging
#[macro_export]
macro_rules! dbg {
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                ::log::debug!("{} = {:#x?}",
                    ::core::stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}

#[macro_export]
macro_rules! newtype_enum {
    (
        $(#[$type_attrs:meta])*
        $visibility:vis enum $type:ident : $base_integer:ty => $(#[$impl_attrs:meta])* {
            $(
                $(#[$variant_attrs:meta])*
                $variant:ident = $value:expr,
            )*
        }
    ) => {
        $(#[$type_attrs])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Eq, PartialEq)]
        $visibility struct $type(pub $base_integer);

        $(#[$impl_attrs])*
        #[allow(unused)]
        impl $type {
            $(
                $(#[$variant_attrs])*
                pub const $variant: $type = $type($value);
            )*
        }

        #[allow(unused)]
        impl core::fmt::Debug for $type {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                match *self {
                    // Display variants by their name, like Rust enums do
                    $(
                        $type::$variant => write!(f, stringify!($variant)),
                    )*

                    // Display unknown variants in tuple struct format
                    $type(unknown) => {
                        write!(f, "{}({})", stringify!($type), unknown)
                    }
                }
            }
        }
    }
}

