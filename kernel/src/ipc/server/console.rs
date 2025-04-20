use core::sync::atomic::{AtomicBool, Ordering};
#[allow(unused_imports)] use crate::prelude::*;
use utils::better_cow::Cow;
use crate::ipc::Error;
use crate::ipc::server::Server;

/// IO server for the kernel debug console
/// 
/// Probably temporary jank until we have userspace drivers working
/// 
/// Provides a single object at `/` implementing `core.io.Read` and `core.io.Write`
/// 
/// The object is internally locked, preventing multiple processes from opening it simultaneously
#[derive(Debug)]
pub struct ConsoleServer {
	lock: AtomicBool,
}

impl Server for ConsoleServer {
	fn open(&self, endpoint: Cow<'_, Box<str>, str>) -> Result<usize, Error> {
		if &*endpoint == "" {
			let res = self.lock.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed);
			if res.is_ok() { Ok(1) }
			else { Err(Error::NameInUse) }
		} else { Err(Error::InvalidArg) }
	}
}

impl ConsoleServer {
	pub const fn new() -> Self {
		Self {
			lock: AtomicBool::new(false),
		}
	}
}
