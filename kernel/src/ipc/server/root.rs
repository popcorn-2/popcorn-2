#[allow(unused_imports)] use crate::prelude::*;
use core::sync::atomic::{AtomicU16, Ordering};
use hashbrown::HashMap;
use kernel_api::sync::Spinlock;
use utils::better_cow::Cow;
use crate::ipc::{Error, server};
use crate::ipc::server::{Server, ServerId};
use super::userspace::UserspaceServer;

#[derive(Debug)]
pub struct RootServer {
	next_handle: AtomicU16,
	handle_map: Spinlock<HashMap<u16, ServerId>>,
}

impl Server for RootServer {
	fn open(&self, endpoint: Cow<'_, Box<str>, str>) -> Result<u16, Error> {
		let path = endpoint.trim_start_matches('/');
		if path.contains('/') { yeet!(Error::InvalidArg); }

		let handle = {
			if self.next_handle.load(Ordering::Relaxed) == u16::MAX { yeet!(Error::Overflow); }
			let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
			handle
		};

		let new_server = UserspaceServer::new_current_thread().into();
		let id = server::servers_mut().insert_server(Cow::Owned(endpoint.into_owned()), new_server)?; // fixme: silly allocation
		
		self.handle_map.lock().try_insert(handle, id)
				.expect("Handle reuse should not happen");
		
		Ok(handle)
	}
}

impl RootServer {
	pub const fn new() -> Self {
		Self {
			next_handle: AtomicU16::new(0),
			handle_map: Spinlock::new(HashMap::with_hasher(hashbrown::hash_map::DefaultHashBuilder::new())),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	
	#[test]
	fn can_return_handle() {
		let server = RootServer::new();
		
		assert_eq!(server.open("/foobar"), 1);
		assert_eq!(server.open("/foobaz"), 2);
	}

	#[test]
	fn duplicate_causes_error() {
		let server = RootServer::new();

		assert_eq!(server.open("/foobar"), 1);
		assert_eq!(server.open("/foobar"), -errors::EADDRINUSE);
	}
	
	#[test]
	fn no_multiple_slashes() {
		let server = RootServer::new();

		assert_eq!(server.open("/foo/bar"), -errors::EINVAL);
		assert_eq!(server.open("/foo/baz"), -errors::EINVAL);
	}
}