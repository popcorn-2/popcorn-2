use core::sync::atomic::{AtomicU32, Ordering};
use hashbrown::HashMap;
use kernel_api::sync::Spinlock;
#[allow(unused_imports)] use crate::prelude::*;
use crate::ipc::{Error, NonNegativeIsize};
use crate::ipc::server::ServerId;

#[derive(Debug)]
pub struct HandleMap {
	map: Spinlock<HashMap<u32, Handle>>,
	next_fd: AtomicU32,
}

impl HandleMap {
	pub fn new() -> Self {
		Self {
			map: Spinlock::new(HashMap::new()),
			next_fd: AtomicU32::new(0),
		}
	}
	
	pub fn push(&self, handle: Handle) -> Result<u32, Error> {
		loop {
			if self.next_fd.load(Ordering::Relaxed) == u32::MAX { panic!("overflow"); } // todo: better
			let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
			
			match self.map.lock().try_insert(fd, handle) {
				Ok(_) => break Ok(fd),
				Err(_) => continue,
			}
		}
	}

	pub fn openat(&self, fd: u32, handle: Handle) -> Result<u32, Error> {
		match self.map.lock().try_insert(fd, handle) {
			Ok(_) => Ok(fd),
			Err(_) => Err(Error::NameInUse),
		}
	}
	
	pub fn get(&self, val: u32) -> Result<Handle, Error> {
		self.map.lock().get(&val).copied().ok_or(Error::InvalidArg)
	}
}

#[derive(Debug, Copy, Clone)]
pub struct Handle {
	server_id: ServerId,
	internal_id: usize,
}

impl Handle {
	pub fn new(server_id: ServerId, internal_id: usize) -> Handle {
		Handle { server_id, internal_id }
	}

	pub fn server_id(&self) -> ServerId { self.server_id }
	pub fn internal_id(&self) -> usize { self.internal_id }
}
