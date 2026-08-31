use alloc::sync::Arc;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicI64, AtomicPtr, AtomicU64, Ordering};
use hashbrown::HashMap;
use log::debug;
use lf_radix_tree::LockFreeRadixTreeU32L4;
use crate::memory::EpochGuard;
use crate::sync::RwSpinlock;
use crate::syscall;
use crate::syscall::Error;
use crate::syscall::server::ServerId;

/// A map of numeric identifiers to [`Handle`]s.
// INVARIANT: `self.0` always comes from `Arc::<HandleMapInner>::into_raw`
#[derive(Debug)]
pub struct HandleMap(AtomicPtr<HandleMapInner>);

struct HandleMapInner {
	map: LockFreeRadixTreeU32L4<Handle>,
	// use a u64 here so that we can detect overflow more easily
	bump_free: AtomicU64,
	// a value of -1 means that there are no handles in the free stack
	// any other value is the next free handle number, which when looked
	// up in the handle map gives, in turn, the next handle in the free stack
	// INVARIANT: value is in -1..=u32::MAX
	// INVARIANT: if holding value in 0..=u32::MAX, then this must point
	//  to an existing entry in the handle map with a value as described
	//  in the above comments
	free_stack_top: AtomicI64,
}

impl HandleMap {
	fn deref<'ebr>(&self, _ebr: &'ebr EpochGuard) -> &'ebr HandleMapInner {
		let ptr = self.0.load(Ordering::Acquire).cast_const();
		// SAFETY: `ptr` is always a valid `HandleMapInner` by invariants of `HandleMap`,
		//  and we hold an ebr guard so the pointed to memory cannot have been dropped yet
		unsafe { &*ptr }
	}

	pub fn clone(&self, _ebr: &EpochGuard) -> Self {
		let ptr = self.0.load(Ordering::Acquire).cast_const();
		// SAFETY: `ptr` is always a valid `HandleMapInner` by invariants of `HandleMap`,
		//  and we hold an ebr guard so the pointed to memory cannot have been dropped yet
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
			map: LockFreeRadixTreeU32L4::new(),
			bump_free: AtomicU64::new(0),
			free_stack_top: AtomicI64::new(-1),
		};
		let arc = Arc::new(this);
		Self(AtomicPtr::new(Arc::into_raw(arc).cast_mut()))
	}

	/// Adds the passed handle to the handle map and returns the position it's placed at.
	///
	/// No guarantees are made on what positions in the map are used.
	///
	/// # Errors
	///
	/// Returns [`Error::Overflow`] if there are no more free positions to add
	/// the handle at.
	pub fn push(&self, handle: Arc<Handle>, ebr: &EpochGuard) -> syscall::Result<u32> {
		self.transfer(PoppedHandle(Arc::into_raw(handle)), ebr)
	}

	/// Adds a [`PoppedHandle`] to the handle map and returns the position it's placed at,
	/// without incurring costs of refcount changes.
	///
	/// No guarantees are made on what positions in the map are used.
	///
	/// # Errors
	///
	/// Returns [`Error::Overflow`] if there are no more free positions to add
	/// the handle at.
	pub fn transfer(&self, handle: PoppedHandle, ebr: &EpochGuard) -> syscall::Result<u32> {
		let this = self.deref(ebr);
		let val = {
			let mut stack_top = this.free_stack_top.load(Ordering::Acquire);
			loop {
				// -1 indicates empty free stack, other values are first value in free stack
				// see comment on `HandleMap::free_stack_top`
				if stack_top < 0 {
					let val = this.bump_free.fetch_add(1, Ordering::Relaxed);
					break u32::try_from(val).map_err(|_| {
						this.bump_free.fetch_sub(1, Ordering::Relaxed);
						Error::Overflow
					})?
				} else {
					let val = stack_top as u32;
					// SAFETY: invariant of `free_stack_top` that it holds existing handle
					let next = unsafe { this.map.get(val).unwrap_unchecked() }
						.load(Ordering::Acquire)
						.addr();
					match this.free_stack_top.compare_exchange_weak(
						stack_top,
						next as i64,
						Ordering::AcqRel,
						Ordering::Acquire,
					) {
						Ok(_) => break val,
						Err(updated) => stack_top = updated,
					}
				}
			}
		};
		let handle = ManuallyDrop::new(handle);
		this.map.insert(val, handle.0.cast_mut()).expect("val from free list must be free");
		Ok(val)
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

	/// Returns a reference to the handle at `val`.
	///
	/// # Errors
	///
	/// Returns [`Error::InvalidHandle`] if the handle is not in the map.
	pub fn get<'ebr>(&self, val: u32, ebr: &'ebr EpochGuard) -> syscall::Result<&'ebr Handle> {
		let this = self.deref(ebr);
		this.map.get(val)
			// SAFETY: holding an EBR guard so if non-null the pointer cannot point to
			//  a dropped Handle
			.and_then(|ptr| unsafe { ptr.load(Ordering::Acquire).as_ref() })
			.ok_or(Error::InvalidHandle)
	}

	/// Removes the handle at `val` from the `HandleMap`, returning a [`PoppedHandle`].
	/// The [`PoppedHandle`] can either be directly dropped, converted into an [`Arc<Handle>`]
	/// (incurrent refcount costs) or directly passed to [`HandleMap::transfer()`] to add it
	/// to a different handle map without incurrent refcount costs.
	///
	/// # Errors
	///
	/// Returns [`Error::InvalidHandle`] if the handle is not in the map.
	#[inline]
	pub fn pop(&self, val: u32, ebr: &EpochGuard) -> syscall::Result<PoppedHandle> {
		let this = self.deref(ebr);
		let ptr = this.map.get(val).ok_or(Error::InvalidHandle)?;
		let ptr = ptr.swap(core::ptr::null_mut(), Ordering::AcqRel).cast_const();
		if ptr.is_null() { return Err(Error::InvalidHandle); }
		Ok(PoppedHandle(ptr))
	}

	pub fn close(&self, val: u32, ebr: &EpochGuard) -> syscall::Result<()> {
		self.pop(val, ebr).map(drop)
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
		let inner = *self.0.get_mut();
		crate::bridge::ebr::defer_and_cleanup(
			// SAFETY: invariant of `self.0` that it comes from `Arc::into_raw`, and no active epoch
			//  holds a reference to this arc anymore
			|ptr| unsafe { drop(Arc::<HandleMapInner>::from_raw(ptr.cast_const().cast())) },
			inner.cast(),
		);
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

/// A wrapper around a [`Handle`] that has been removed from a [`HandleMap`] but not yet dropped.
///
/// This exists to allow a faster path when moving a handle between two maps,
/// by delaying any refcount adjustments on a popped handle until the wrapper is
/// dropped.
// INVARIANT: `self.0` always comes from `Arc::<HandleMapInner>::into_raw`
#[repr(transparent)]
pub struct PoppedHandle(*const Handle);

impl PoppedHandle {
	#[must_use = "dropping PoppedHandle directly cleans up the contained Handle without paying refcount costs"]
	pub fn to_arc(self) -> Arc<Handle> {
		// SAFETY: invariant of `self.0` that it comes from `Arc::into_raw`
		unsafe { Arc::increment_strong_count(self.0) };
		// SAFETY: invariant of `self.0` that it comes from `Arc::into_raw`
		unsafe { Arc::from_raw(self.0) }
	}
}

impl Drop for PoppedHandle {
	fn drop(&mut self) {
		crate::bridge::ebr::defer_and_cleanup(
			// SAFETY: invariant of `self.0` that it comes from `Arc::into_raw`, and no active epoch
			//  holds a reference to this arc anymore
			|ptr| unsafe { drop(Arc::<Handle>::from_raw(ptr.cast_const().cast())) },
			self.0.cast_mut().cast(),
		);
	}
}
