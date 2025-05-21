use ::core::sync::atomic::{AtomicBool, Ordering};
use kernel_api::dbg;
#[allow(unused_imports)] use crate::prelude::*;
use utils::better_cow::Cow;
use crate::hal::FormatWriter;
use crate::ipc::{Error, NonNegativeIsize};
use crate::ipc::server::Server;
use super::super::core_protos;

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

	fn dispatch_vs_ve(&self, proto_method: u128, fd: usize, b: usize, s: String) -> Result<NonNegativeIsize, Error> {
		if fd != 1 { return Err(Error::Unimplemented); }
		
		match proto_method {
			m if m == const { core_protos::io::WRITE | core_protos::io::WRITE_WRITE } => {
				sprint!("{s}");
				Ok(NonNegativeIsize::new(s.len() as isize).unwrap())
			}
			_ => unimplemented!()
		}
	}

	fn dispatch_vM_ve(&self, proto_method: u128, fd: usize, b: usize, size: usize) -> Result<(Box<[u8]>, NonNegativeIsize), Error> {
		if fd != 1 { return Err(Error::Unimplemented); }

		match proto_method {
			m if m == const { core_protos::io::READ | core_protos::io::READ_READ } => {
				let mut buf = Box::new_uninit_slice(size);
				for i in 0..size {
					buf[i].write(crate::hal::SerialOut::read());
				}
				dbg!(Ok((unsafe { buf.assume_init() }, NonNegativeIsize::new(size as isize).unwrap())))
			}
			_ => unimplemented!()
		}
	}
}

impl ConsoleServer {
	pub const fn new() -> Self {
		Self {
			lock: AtomicBool::new(false),
		}
	}
}
