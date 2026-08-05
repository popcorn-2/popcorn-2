//! Kernel modules.
//!
//! # Examples
//!
//! ```
//! use kernel_api::modules::{kernel_module, Module};
//! use kernel_api::sync::OnceLock;
//!
//! struct MyModuleContext {
//!     // ...
//! # foo: i32,
//! }
//!
//! static CONTEXT: OnceLock<MyModuleContext> = OnceLock::new();
//!
//! kernel_module! {
//!     type: MyModuleContext,
//!     name: "My Custom Module",
//!     author: "Popcorn Contributors",
//!     license: "MPL-2.0",
//! }
//!
//! impl Module for MyModuleContext {
//!     fn early_init() -> Result<(), &'static str> {
//!          debug!("early init of `My Custom Module`");
//!          Ok(())
//!     }
//!
//!     fn late_init() -> Result<(), &'static str> {
//!          debug!("late init of `My Custom Module`");
//!          // initialise context
//!          CONTEXT.get_or_init(Self::new);
//!          Ok(())
//!     }
//! }
//!
//! impl MyModuleContext {
//!     fn new() -> Self {
//!         // ...
//! #       Self { foo: 5 }
//!     }
//! }
//! ```

/// Registers a kernel module with the kernel.
///
/// The `type` argument should be a type implementing the [`Module`] trait.
///
/// Additionally, it also takes the following arguments in order:
/// - `name` - a user facing name for the module
/// - `description` (optional) - a user facing description of the module
/// - `author` (optional) - the author of the module
/// - `license` - An SPDX license identifier (currently only `MPL-*` is accepted)
///
/// # Examples
///
/// See the [module-level documentation](self).
#[expect(rustdoc::missing_doc_code_examples, reason = "documented at module level")]
#[macro_export]
macro_rules! kernel_module {
    (
	    type: $ty:ty,
	    name: $name:literal,
	    $(description: $description:literal,)?
	    $(author: $author:literal,)?
	    license: $license:tt$(,)?
    ) => {
	    const _: () = {
		    $crate::kernel_module_license!(@license $license);
		    const fn __assert_module_implements_module<T: $crate::modules::Module>() {}
		    __assert_module_implements_module::<$ty>();

		    impl $crate::modules::__private::RequiresAbiShim for $ty {}
	    };
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! kernel_module_license {
	(@license "MPL-1.0") => {};
	(@license "MPL-1.1") => {};
	(@license "MPL-2.0") => {};
}

/// The top level trait for a kernel module.
///
/// This is to be implemented on a ZST to allow the module to hook into
/// various points during kernel boot.
///
/// # Examples
///
/// See the [module-level documentation](self).
#[expect(rustdoc::missing_doc_code_examples, reason = "documented at module level")]
pub trait Module {
    /// Called immediately upon kernel start.
    ///
    /// At this point many kernel features are unusable, such as
    /// threading, async tasks, and memory allocation.
    #[expect(clippy::missing_errors_doc, reason = "errors depend on implementor")]
    #[expect(rustdoc::missing_doc_code_examples, reason = "not called by end users")]
    fn early_init() -> Result<(), &'static str> { Ok(()) }

    /// Called once the kernel is ready to start userspace applications.
    #[expect(clippy::missing_errors_doc, reason = "errors depend on implementor")]
    #[expect(rustdoc::missing_doc_code_examples, reason = "not called by end users")]
    fn late_init() -> Result<(), &'static str> { Ok(()) }
}
