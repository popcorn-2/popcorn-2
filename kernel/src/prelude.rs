#![allow(unused_imports)]

#[doc(no_inline)]
pub use crate::{sprint, sprintln, yeet};
#[doc(no_inline)]
pub(crate) use crate::percpu::percpu_v2;

#[doc(no_inline)]
pub use crate::io_ext::IoExt as _;
#[doc(no_inline)]
pub use crate::memory::r#virtual::AddressSpaceExt as _;
#[doc(no_inline)]
pub use crate::ipc::HandleExt as _;

#[doc(no_inline)]
pub use kernel_api::prelude::*;
