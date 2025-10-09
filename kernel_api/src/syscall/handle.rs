use alloc::sync::Arc;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use hashbrown::HashMap;
use log::debug;
use crate::sync::{RwSpinlock, Spinlock};
use crate::syscall;
use crate::syscall::Error;
use crate::syscall::server::ServerId;

#[derive(Debug)]
pub struct HandleMap(AtomicPtr<HandleMapInner>);

impl Clone for HandleMap {
	fn clone(&self) -> Self {
		let ptr = self.0.load(Ordering::SeqCst).cast_const();
		unsafe { Arc::increment_strong_count(ptr) };
		Self(AtomicPtr::new(ptr.cast_mut()))
	}
}

#[derive(Debug)]
pub struct HandleMapInner {
	map: Spinlock<HashMap<u32, Arc<Handle>>>,
	next_fd: AtomicU32,
}

impl HandleMap {
	fn deref(&self) -> &HandleMapInner {
		let ptr = self.0.load(Ordering::SeqCst).cast_const();
		unsafe { &*ptr }
	}

	pub fn new() -> Self {
		let this = HandleMapInner {
			map: Spinlock::new(HashMap::new()),
			next_fd: AtomicU32::new(3),
		};
		let arc = Arc::new(this);
		Self(AtomicPtr::new(Arc::into_raw(arc).cast_mut()))
	}

	pub fn push(&self, handle: Arc<Handle>) -> syscall::Result<u32> {
		let this = self.deref();
		loop {
			if this.next_fd.load(Ordering::Relaxed) == u32::MAX { return Err(Error::Overflow); }
			let fd = this.next_fd.fetch_add(1, Ordering::Relaxed);

			match this.map.lock().try_insert(fd, handle.clone()) {
				Ok(_) => break Ok(fd),
				Err(_) => continue,
			}
		}
	}

	pub fn openat(&self, fd: u32, handle: Arc<Handle>) -> Result<u32, Error> {
		let this = self.deref();
		match this.map.lock().try_insert(fd, handle) {
			Ok(_) => Ok(fd),
			Err(_) => Err(Error::NameInUse),
		}
	}

	pub fn get(&self, val: u32) -> Result<Arc<Handle>, Error> {
		let this = self.deref();
		this.map.lock().get(&val).cloned().ok_or(Error::InvalidHandle)
	}

	pub fn pop(&self, val: u32) -> Result<Arc<Handle>, Error> {
		let this = self.deref();
		this.map.lock().remove(&val).ok_or(Error::InvalidHandle)
	}

	pub unsafe fn swap(&self, other: HandleMap, ordering: Ordering) -> HandleMap {
		let other = ManuallyDrop::new(other);
		let other = other.0.load(ordering);
		let old = self.0.swap(other, ordering);
		HandleMap(AtomicPtr::new(old))
	}
}

#[derive(Debug)]
pub struct Handle {
	#[doc(hidden)]
	pub __protocols: RwSpinlock<ManuallyDrop<HashMap<u128, (ServerId, isize)>>>,
}

impl Handle {
	pub fn new(server_id: ServerId, internal_id: isize, protocols: &[u128]) -> Arc<Handle> {
		Arc::new(Handle {
			__protocols: RwSpinlock::new(ManuallyDrop::new(
				HashMap::from_iter(protocols.iter().copied().zip(core::iter::repeat((server_id, internal_id))))
			)),
		})
	}

	pub fn id(&self, protocol: u128) -> Result<(ServerId, isize), Error> {
		self.__protocols.read().get(&protocol).copied().ok_or(Error::UnsupportedProtocol)
	}

	pub fn has_protocols(&self, protocols: &[u128]) -> bool {
		debug!("check handle {self:#x?} for protocols {protocols:#x?}");
		protocols.iter().all(|uid| self.__protocols.read().contains_key(uid))
	}

	pub fn merge(&self, other: &Self) -> Result<(), Error> {
		let mut guard = self.__protocols.write();
		if guard.keys().any(|uid| other.__protocols.read().contains_key(uid)) {
			debug!("overlap of protocol");
			return Err(Error::ProtocolOverlap);
		}
		guard.extend(other.__protocols.read().iter());
		Ok(())
	}
}

impl Drop for Handle {
	fn drop(&mut self) {
		crate::bridge::handle::drop(self);
	}
}
