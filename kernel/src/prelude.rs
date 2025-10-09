#![allow(unused_imports)]

// Matches the alloc subset of the std prelude
pub use alloc::borrow::ToOwned;
pub use alloc::boxed::Box;
pub use alloc::string::{String, ToString};
pub use alloc::vec::Vec;
pub use alloc::vec;
pub use alloc::format;

// Commonly used kernel imports
pub use log::{error, warn, info, debug, trace};
pub use crate::{sprint, sprintln, yeet};
pub(crate) use crate::percpu::percpu_v2;

pub use crate::io_ext::IoExt as _;
pub use crate::memory::r#virtual::AddressSpaceExt as _;
pub use crate::ipc::HandleExt as _;

pub use core::prelude::rust_2024::*;
