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
			let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
			if fd == u32::MAX { return Err(Error::Overflow); }
			
			match self.map.lock().try_insert(fd, handle) {
				Ok(_) => break Ok(fd),
				Err(_) => continue,
			}
		}
	}
	
	pub fn get(&self, val: u32) -> Result<Handle, Error> {
		self.map.lock().get(&val).copied().ok_or(Error::InvalidArg)
	}
}

#[derive(Debug, Copy, Clone)]
pub struct Handle {
	server_id: ServerId,
	internal_id: u16,
}

impl Handle {
	pub fn new(server_id: ServerId, internal_id: u16) -> Handle {
		Handle { server_id, internal_id }
	}
	
	pub fn from_arg(arg: usize) -> Result<Self, Error> {
		const HALF_BITS: usize = core::mem::size_of::<usize>() * 8 / 2;
		let lower = arg & !(1 << HALF_BITS);
		let upper = arg >> HALF_BITS;
		
		let server_id = ServerId::new(
			lower.try_into().map_err(|_| Error::InvalidArg)?
		);
		let internal_id = upper.try_into().map_err(|_| Error::InvalidArg)?;
		Ok(Self {
			server_id,
			internal_id,
		})
	}

	pub fn to_arg(self) -> NonNegativeIsize {
		const HALF_BITS: usize = core::mem::size_of::<usize>() * 8 / 2;
		let lower = usize::from(self.server_id.get());
		let upper = usize::from(self.internal_id);
		
		NonNegativeIsize::new((upper << HALF_BITS | lower) as isize)
				.expect("Handle should have ")
	}
}
