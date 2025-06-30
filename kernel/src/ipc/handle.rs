use alloc::sync::Arc;
use core::sync::atomic::{AtomicU32, Ordering};
use hashbrown::{HashMap, HashSet};
use kernel_api::sync::Spinlock;
use kernel_api::time::Instant;
use crate::ipc::Error;

struct HandleDescriptor {
	server: ServerId,
	handle_id: isize,
	supported_protocols: HashSet<u128>,
}

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub struct ServerId(usize, u64);

impl ServerId {
	pub const MAX: usize = u16::MAX as usize;

	pub fn get(self) -> u16 {
		debug_assert!(self.0 <= Self::MAX, "ServerId should not be above ServerId::MAX");
		self.0 as u16
	}

	pub fn new(val: u16) -> Self { Self(val.into(), Instant::now().get() as u64) } // assuming arch val is clock cycles, still takes >100 years to overflow generation number creating a new server every clock cycle at 5GHz
}

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

			match self.map.lock().try_insert(fd, handle.clone()) {
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
		self.map.lock().get(&val).cloned().ok_or(Error::InvalidHandle)
	}
}

#[derive(Debug, Clone)]
pub struct Handle {
	server_id: ServerId,
	internal_id: isize,
	supported_protocols: Arc<HashSet<u128>>,
}

impl Handle {
	pub fn new(server_id: ServerId, internal_id: isize, protocols: &[u128]) -> Handle {
		Handle {
			server_id,
			internal_id,
			supported_protocols: Arc::new(HashSet::from_iter(protocols.iter().copied())),
		}
	}

	pub fn server_id(&self) -> ServerId { self.server_id }
	pub fn internal_id(&self) -> isize { self.internal_id }
	
	pub fn has_protocols(&self, protocols: &[u128]) -> bool {
		protocols.iter().all(|uid| self.supported_protocols.contains(uid))
	}
}

