use alloc::sync::Arc;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicPtr, Ordering};
use hashbrown::HashMap;
use log::{debug, warn};
use slab::Slab;
use crate::memory::EpochGuard;
use crate::sync::{RwSpinlock, Spinlock};
use crate::syscall;
use crate::syscall::Error;
use crate::syscall::server::ServerId;

/// A map of numeric identifiers to [`Handle`]s.
// INVARIANT: `self.0` always comes from `Arc::<HandleMapInner>::into_raw`
#[derive(Debug)]
pub struct HandleMap(AtomicPtr<HandleMapInner>);

#[derive(Debug)]
struct HandleMapInner {
	map: Spinlock<Slab<Arc<Handle>>>,
}

impl HandleMap {
	fn deref<'ebr>(&self, _ebr: &'ebr EpochGuard) -> &'ebr HandleMapInner {
		let ptr = self.0.load(Ordering::Acquire).cast_const();
		// SAFETY: `ptr` is always a valid `HandleMapInner` by invariants of `HandleMap`.
		unsafe { &*ptr }
	}

	pub fn clone(&self, _ebr: &EpochGuard) -> Self {
		let ptr = self.0.load(Ordering::Acquire).cast_const();
		// SAFETY: `ptr` always comes from `Arc::into_raw` by `HandleMap` invariants.
		unsafe { Arc::increment_strong_count(ptr) };
		Self(AtomicPtr::new(ptr.cast_mut()))
	}

	/// Creates an empty `HandleMap`.
	///
	/// # Examples
	///
	/// ```
	/// let map = HandleMap::new();
	/// ```
	#[must_use]
	pub fn new() -> Self {
		let this = HandleMapInner {
			map: Spinlock::new(Slab::new()),
		};
		let arc = Arc::new(this);
		Self(AtomicPtr::new(Arc::into_raw(arc).cast_mut()))
	}

	/// Adds the passed handle to the handle map and returns the position it's placed at.
	///
	/// No guarantees are made on what positions in the map are used.
	/// If a specific position is needed, use [`openat()`](Self::openat).
	///
	/// # Errors
	///
	/// Returns [`Error::Overflow`] if there are no more free positions to add
	/// the handle at.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::syscall::handle::{Handle, HandleMap};
	/// use kernel_api::syscall::server::ServerId;
	///
	/// let mut map = HandleMap::new();
	/// let handle = Handle::new(ServerId::INVALID, 0, &[], "");
	///
	/// let fd = map.push(handle)?;
	/// assert!(map.pop(fd).is_ok());
	/// # Ok::<(), kernel_api::syscall::Error>(())
	/// ```
	pub fn push(&self, handle: Arc<Handle>, ebr: &EpochGuard) -> syscall::Result<u32> {
		let this = self.deref(ebr);
		let mut guard = this.map.lock();
		if guard.len() >= (u32::MAX as usize) { return Err(Error::Overflow); }

		let fd = guard.insert(handle);
		Ok(fd as u32)
	}

	/// Adds the passed handle to the handle map at the specified position.
	///
	/// Returns the position if successful, or an error if the specified position is
	/// already in use.
	///
	/// # Errors
	///
	/// Returns [`Error::NameInUse`] if the specified position is already in
	/// use.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::syscall::handle::{Handle, HandleMap};
	/// use kernel_api::syscall::server::ServerId;
	///
	/// let mut map = HandleMap::new();
	/// let handle = Handle::new(ServerId::INVALID, 0, &[], "");
	///
	/// let fd = map.openat(5, handle)?;
	/// assert_eq!(fd, 5);
	/// # Ok::<(), kernel_api::syscall::Error>(())
	/// ```
	#[deprecated = "`openat` no longer supported - use `push` and don't rely on fixed handle numbers"]
	pub fn openat(&self, _val: u32, _handle: Arc<Handle>) -> syscall::Result<u32> {
		Err(Error::FutureCompat)
	}

	/// Duplicates and returns the handle at `val`.
	///
	/// # Errors
	///
	/// Returns [`Error::InvalidHandle`] if the handle is not in the map.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::syscall::handle::{Handle, HandleMap};
	/// use kernel_api::syscall::server::ServerId;
	/// use alloc::sync::Arc;
	///
	/// let mut map = HandleMap::new();
	/// let handle = Handle::new(ServerId::INVALID, 0, &[], "");
	///
	/// let fd = map.push(Arc::clone(handle))?;
	/// let new_handle = map.get(fd).unwrap();
	///
	/// assert!(Arc::ptr_eq(&handle, &new_handle));
	/// # Ok::<(), kernel_api::syscall::Error>(())
	/// ```
	pub fn get(&self, val: u32, ebr: &EpochGuard) -> syscall::Result<Arc<Handle>> {
		let this = self.deref(ebr);
		this.map.lock().get(val as usize).cloned().ok_or(Error::InvalidHandle)
	}

	/// Removes the handle at `val` from the `HandleMap`.
	///
	/// # Errors
	///
	/// Returns [`Error::InvalidHandle`] if the handle is not in the map.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::syscall;
	/// use kernel_api::syscall::handle::{Handle, HandleMap};
	/// use kernel_api::syscall::server::ServerId;
	/// use alloc::sync::Arc;
	///
	/// let mut map = HandleMap::new();
	/// let handle = Handle::new(ServerId::INVALID, 0, &[], "");
	///
	/// let fd = map.push(Arc::clone(handle))?;
	///
	/// let new_handle = map.pop(fd).unwrap();
	/// assert_eq!(map.pop(fd), syscall::Error::InvalidHandle);
	///
	/// assert!(Arc::ptr_eq(&handle, &new_handle));
	/// # Ok::<(), syscall::Error>(())
	/// ```
	pub fn pop(&self, val: u32, ebr: &EpochGuard) -> syscall::Result<Arc<Handle>> {
		let this = self.deref(ebr);
		this.map.lock().try_remove(val as usize).ok_or(Error::InvalidHandle)
	}

	/// Swaps `self` to contain the mappings contained in `other`, and
	/// returns the mappings originally contained in `self`.
	///
	/// <div class="warning">
	///
	/// This will race with any calls on `self` to `pop`, `push`, or `get`
	/// such that changes may modify either the old or new handle map.
	/// Therefore this method should be avoided when `self` is shared
	/// between multiple threads and concurrently modified.
	///
	/// </div>
	#[must_use]
	pub fn swap(&self, other: Self) -> Self {
		let mut other = ManuallyDrop::new(other);
		let other_ptr = *other.0.get_mut();
		let old = self.0.swap(other_ptr, Ordering::AcqRel);
		Self(AtomicPtr::new(old))
	}
}

impl Default for HandleMap {
	fn default() -> Self {
		Self::new()
	}
}

impl Drop for HandleMap {
	fn drop(&mut self) {
		let inner = self.0.load(Ordering::Acquire);
		warn!("HandleMapInner leaked - retire not yet implemented outside of kernel");
	}
}

/// A handle representing a userspace resource within the kernel.
#[derive(Debug)]
#[expect(clippy::partial_pub_fields, reason = "kernel needs access to __protocols for drop internals")]
pub struct Handle {
	#[doc(hidden)]
	pub __protocols: RwSpinlock<ManuallyDrop<HashMap<u128, (ServerId, isize)>>>,
	endpoint: Arc<str>,
}

impl Handle {
	/// Creates a new `Handle` object pointing to the passed server and object ID, and supporting the passed list of protocols.
	pub fn new(server_id: ServerId, internal_id: isize, protocols: &[u128], endpoint: impl Into<Arc<str>>) -> Arc<Self> {
		Arc::new(Self {
			__protocols: RwSpinlock::new(ManuallyDrop::new(
				protocols.iter().copied().zip(core::iter::repeat((server_id, internal_id))).collect()
			)),
			endpoint: endpoint.into(),
		})
	}

	/// Provides the server and object ID that this handle modifies for methods on the passed protocol.
	///
	/// # Errors
	///
	/// Returns [`Error::UnsupportedProtocol`] if the handle doesn't
	/// support the requested protocol.
	pub fn id(&self, protocol: u128) -> syscall::Result<(ServerId, isize)> {
		self.__protocols.read().get(&protocol).copied().ok_or(Error::UnsupportedProtocol)
	}

	/// Returns `true` if the handle supports all the protocols passed.
	#[must_use]
	pub fn has_protocols(&self, protocols: &[u128]) -> bool {
		debug!("check handle {self:#x?} for protocols {protocols:#x?}");
		protocols.iter().all(|uid| self.__protocols.read().contains_key(uid))
	}

	/// Combines the current `Handle` with `other`.
	///
	/// This results in all methods on `other` also being available
	/// on `self`, and they access the same underlying object.
	///
	/// # Errors
	///
	/// Returns [`Error::ProtocolOverlap`] if `other` supports a protocol
	/// that `self` already supports.
	pub fn merge(&self, other: &Self) -> syscall::Result<()> {
		let mut guard = self.__protocols.write();
		if guard.keys().any(|uid| other.__protocols.read().contains_key(uid)) {
			debug!("overlap of protocol");
			return Err(Error::ProtocolOverlap);
		}
		guard.extend(other.__protocols.read().iter());
		Ok(())
	}
	
	/// Returns the endpoint (as could be passed to an `abi_v1::open()` syscall) this handle points to.
	#[deprecated = "popcorn handles are being reworked in such a way that endpoints will no exist"]
	pub const fn endpoint(&self) -> &Arc<str> {
		&self.endpoint
	}
}

impl Drop for Handle {
	fn drop(&mut self) {
		crate::bridge::handle::drop(self);
	}
}
