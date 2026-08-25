use core::fmt;
use crate::allocator::AllocError;
use crate::ptr::PointerError;

/// A specialized [`Result`](`core::result::Result`) type for syscalls.
pub type Result<T> = core::result::Result<T, Error>;

macro_rules! define_error {
    ($(#[$attr:meta])* pub enum Error {
	    $($(#[$item_attr:meta])* $name:ident = $val:literal),*
	    $(,)?
    }) => {
	    $(#[$attr])* pub enum Error {
	        $($(#[$item_attr])* $name = $val),*
        }

	    impl From<u128> for Error {
			fn from(value: u128) -> Error {
				match value {
					$($val => Error::$name),*,
					_ => Self::Invalid,
				}
			}
		}
    };
}

define_error! {
	/// Types of error that can be returned from a syscall.
	#[derive(Debug, Copy, Clone, Eq, PartialEq)]
	#[repr(u16)]
	#[non_exhaustive]
	pub enum Error {
		/// Pointer argument passed was invalid.
		InvalidPointer = 0,
		/// String argument passed was malformed UTF-8.
		InvalidUtf8 = 1,
		/// Server or handle does not support requested protocol or method.
		UnsupportedProtocol = 2,
		/// Unknown protocol or method.
		UnknownProtocol = 3,
		/// Requested endpoint name is invalid or does not exist.
		InvalidEndpoint = 4,
		/// Requested endpoint name is already in use.
		NameInUse = 5,
		/// Handle argument passed was invalid.
		InvalidHandle = 6,
		/// Too many open objects.
		Overflow = 7,
		/// Server is no longer running.
		DeadServer = 9,
		/// Incorrect return type for called method.
		InvalidReturn = 10,
		/// Generic error.
		Invalid = 11,
		/// Failed to allocate memory.
		AllocationFailure = 12,
		/// Argument passed is invalid.
		InvalidArg = 13,
		/// Requested method does not support being called asynchronously.
		AsyncUnsupported = 14,
		/// Passed argument value is reserved for future use.
		FutureCompat = 15,
		/// Object is not long enough for passed arguments.
		EndOfData = 16,
		/// Protocol already exists on handle.
		ProtocolOverlap = 17,
	}
}

impl From<PointerError> for Error {
	fn from(_: PointerError) -> Self {
		Self::InvalidPointer
	}
}

impl From<AllocError> for Error {
	fn from(_: AllocError) -> Self {
		Self::AllocationFailure
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "syscall failed")
	}
}

impl core::error::Error for Error {}
