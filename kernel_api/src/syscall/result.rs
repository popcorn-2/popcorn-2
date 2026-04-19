use crate::allocator::AllocError;
use crate::ptr::PointerError;

pub type Result<T> = core::result::Result<T, Error>;

macro_rules! define_error {
    ($(#[$attr:meta])* pub enum Error {
	    $($name:ident = $val:literal),*
	    $(,)?
    }) => {
	    $(#[$attr])* pub enum Error {
	        $($name = $val),*
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
	#[derive(Debug, Copy, Clone, Eq, PartialEq)]
	#[repr(u16)]
	pub enum Error {
		InvalidPointer = 0,
		InvalidUtf8 = 1,
		UnsupportedProtocol = 2,
		UnknownProtocol = 3,
		EndpointNotFound = 4,
		NameInUse = 5,
		InvalidHandle = 6,
		Overflow = 7,
		InvalidName = 8,
		DeadServer = 9,
		InvalidReturn = 10,
		Invalid = 11,
		AllocationFailure = 12,
		InvalidArg = 13,
		AsyncUnsupported = 14,
		FutureCompat = 15,
		EndOfData = 16,
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
