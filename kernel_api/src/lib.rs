//! Popcorn2 kernel public API.
//! 
//! Provides a mix of utility types and functions to replace the lack of `std`, as well as a stable API to kernel internals.

#![feature(min_specialization)]
#![feature(step_trait)]
#![feature(doc_cfg)]
#![feature(arbitrary_self_types_pointers)]
#![feature(cfg_target_has_atomic)]
#![feature(const_trait_impl)]
#![feature(const_convert)]
#![feature(const_ops)]
#![feature(const_option_ops)]
#![feature(derive_const)]
#![feature(const_cmp)]
#![feature(const_clone)]
#![feature(strict_provenance_lints)]
#![feature(decl_macro)]
#![cfg_attr(doc, feature(intra_doc_pointers))]
#![cfg_attr(doc, feature(rustdoc_missing_doc_code_examples))]
#![feature(prelude_import)]

#![cfg_attr(all(feature = "full", target_has_atomic_load_store = "128"), feature(integer_atomics))]

#![cfg_attr(feature = "full", feature(type_changing_struct_update))]
#![cfg_attr(feature = "full", feature(unsize))]
#![cfg_attr(feature = "full", feature(dispatch_from_dyn))]
#![cfg_attr(feature = "full", feature(coerce_unsized))]
#![cfg_attr(feature = "full", feature(pointer_is_aligned_to))]
#![cfg_attr(feature = "full", feature(slice_ptr_get))]
#![cfg_attr(feature = "full", feature(ptr_metadata))]
#![cfg_attr(feature = "full", feature(maybe_uninit_fill))]
#![cfg_attr(feature = "full", feature(sanitize))]
#![cfg_attr(feature = "full", feature(negative_impls))]
#![cfg_attr(feature = "full", feature(const_destruct))]
#![cfg_attr(feature = "full", feature(never_type))]
#![cfg_attr(feature = "full", feature(debug_closure_helpers))]
#![cfg_attr(feature = "full", feature(const_default))]
#![cfg_attr(all(feature = "full", not(feature = "use_std")), feature(drop_guard))]

#![cfg_attr(feature = "full", expect(unstable_name_collisions, reason = "custom truncate extension causes issues"))]

#![expect(internal_features, reason = "prelude import")]

#![cfg_attr(not(any(feature = "use_std", doc)), no_std)]

#![doc(auto_cfg(hide(feature = "full", feature = "use_std")))]

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

#[cfg(feature = "full")]
pub mod modules;

pub mod int_extension;

pub mod prelude;

mod prelude_import {
	#![expect(clippy::allow_attributes, reason = "unknown if all prelude will be used or not")]

	// The compiler expects the prelude definition to be defined before it's use statement
	#[prelude_import]
	#[allow(unused_imports, reason = "prelude import")]
	pub use crate::prelude::*;
}

#[cfg(feature = "full")]
mod sealed {
    pub trait Sealed {}
}

/// Prints and returns the value of a given expression for quick and dirty debugging.
///
/// # Examples
///
/// ```
/// # fn complex_operation() -> i8 { 5 }
/// // prints `complex_operation() = ...` and assigns the result to `x`
/// let x = dbg!(complex_operation());
/// ```
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

/// Creates a struct newtype from an enum definition.
///
/// The enum is represented as a tuple struct with a single field of type `base_integer`.
/// Each variant is converted to an associated constant of the same name, with value equal
/// to `$type($value)`.
///
/// This allows for representing enums that do not have a fixed set of variants, as creating
/// an enum in Rust with an out of range discriminant is undefined behaviour.
///
/// # Examples
///
/// ```
/// use kernel_api::newtype_enum;
///
/// newtype_enum! {
///     /// The type of food requested.
///     pub enum FoodType: pub u8 => {
///         /// Red and round.
///         TOMATO = 0,
///         /// Green and cylindrical.
///         CUCUMBER = 1,
///     }
/// }
///
/// assert_eq!(FoodType::TOMATO, FoodType(0));
/// assert_eq!(FoodType::CUCUMBER, FoodType(1));
/// let carrot = FoodType(2);
/// ```
#[macro_export]
macro_rules! newtype_enum {
    (
        $(#[$type_attrs:meta])*
        $visibility:vis enum $type:ident : $base_vis:vis $base_integer:ty => {
            $(
                $(#[$variant_attrs:meta])*
                $variant:ident = $value:expr,
            )*
        }
    ) => {
        $(#[$type_attrs])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Eq, PartialEq)]
        $visibility struct $type($base_vis $base_integer);

        impl $type {
            $(
                $(#[$variant_attrs])*
                pub const $variant: $type = $type($value);
            )*
        }

        impl core::fmt::Debug for $type {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
