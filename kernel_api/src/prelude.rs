//! The Popcorn kernel prelude.
//!
//! This is similar to the [std prelude](`std::prelude`) and reexports as many of the
//! same items as possible. Additionally, several kernel specific traits and macros
//! which are commonly used are included.

// Matches the alloc subset of the std prelude
#[doc(no_inline)]
pub use alloc::borrow::ToOwned;
#[doc(no_inline)]
pub use alloc::boxed::Box;
#[doc(no_inline)]
pub use alloc::string::{String, ToString};
#[doc(no_inline)]
pub use alloc::vec::Vec;
#[doc(no_inline)]
pub use alloc::vec;
#[doc(no_inline)]
pub use alloc::format;

// Commonly used kernel imports
#[doc(no_inline)]
pub use log::{error, warn, info, debug, trace};

#[doc(no_inline)]
pub use crate::int_extension::Truncate;

#[doc(no_inline)]
pub use core::prelude::rust_2024::*;
