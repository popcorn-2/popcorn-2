//! Provides types for interacting with system calls.

#[deprecated = "use the items in the `syscall` module root instead"]
pub mod handle {
	//! Provides types for working with userspace resources.
	pub use super::handle_inner::*;
}

#[deprecated = "use the items in the `syscall` module root instead"]
pub mod server {
	//! Provides types for implementing servers.
	pub use super::server_inner::*;
}

mod result;
mod handle_inner;
mod server_inner;

pub use result::*;
pub use handle_inner::*;
pub use server_inner::*;

use crate::channel;
use crate::channel::Receiver;

/// A map of syscall keys to completed results.
///
/// This is a thin wrapper around a [`channel`].
///
/// # Examples
///
/// ```
/// # fn open() -> kernel_api::syscall::Result<u128>;
/// # fn read() -> kernel_api::syscall::Result<u128>;
/// # fn close() -> kernel_api::syscall::Result<u128>;
/// use kernel_api::syscall::AsyncMap;
///
/// // create a new map to store result in
/// let map = AsyncMap::new();
///
/// // process some syscalls and store the results
/// let result = open(/* ... */);
/// map.push_result(1, result);
///
/// let result = read(/* ... */);
/// map.push_result(2, result);
///
/// let result = close(/* ... */);
/// map.push_result(3, result);
/// ```
#[derive(Debug)]
pub struct AsyncMap {
	#[doc(hidden)] pub queue: Receiver<(usize, Result<u128>)>,
}

impl AsyncMap {
	/// Creates a new empty `AsyncMap`.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::syscall::AsyncMap;
	///
	/// let map = AsyncMap::new();
	/// ```
	#[must_use]
	pub fn new() -> Self {
		Self {
			queue: channel::unbounded().1,
		}
	}

	/// Adds a new result with the given `key` to the map.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::syscall::AsyncMap;
	///
	/// let mut map = AsyncMap::new();
	/// map.push_result(5, Ok(0));
	/// ```
	pub fn push_result(&self, key: usize, result: Result<u128>) {
		self.queue.push((key, result));
	}
}

impl Default for AsyncMap {
	fn default() -> Self {
		Self::new()
	}
}
