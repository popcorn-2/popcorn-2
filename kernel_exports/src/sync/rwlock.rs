use core::cell::UnsafeCell;
use core::fmt::Formatter;
use core::mem;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering, AtomicUsize};
use crate::sync::{Flags, TryLockError};
use super::{LockResult, PoisonError, TryLockResult, reset_interrupts, disable_interrupts};

struct RwCount(AtomicUsize);

impl RwCount {
	const WRITE_BIT_MASK: usize = 1<<(mem::size_of::<usize>() * 8 - 1);
	const READ_COUNT_MASK: usize = !Self::WRITE_BIT_MASK;

	const fn new() -> Self { Self(AtomicUsize::new(0)) }

	fn try_lock_read(&self) -> bool {
		let mut old_value = self.0.load(Ordering::Acquire);
		loop {
			if old_value == Self::READ_COUNT_MASK { panic!("Reader count overflowed") }
			if (old_value & Self::WRITE_BIT_MASK) != 0 { return false; }

			match self.0.compare_exchange_weak(old_value, old_value + 1, Ordering::Acquire, Ordering::Acquire) {
				Ok(_) => return true,
				Err(new_old_value) => old_value = new_old_value
			}
		}
	}

	fn try_lock_write(&self) -> bool {
		self.0.compare_exchange_weak(0, Self::WRITE_BIT_MASK, Ordering::Acquire, Ordering::Acquire)
				.is_ok()
	}

	fn spin_lock_write(&self) {
		while !self.try_lock_write() {}
	}

	fn spin_lock_read(&self) {
		while !self.try_lock_read() {}
	}

	fn unlock_write(&self) {
		if cfg!(debug_assertions) {
			self.0.compare_exchange(Self::WRITE_BIT_MASK, 0, Ordering::Release, Ordering::Acquire)
					.expect("BUG: RwLock writer dropped while readers were active");
		} else {
			self.0.store(0, Ordering::Release);
		}
	}

	fn unlock_read(&self) {
		let mut old_count = self.0.load(Ordering::Acquire);
		loop {
			if cfg!(debug_assertions) && (old_count & Self::WRITE_BIT_MASK != 0) {
				panic!("BUG: RwLock reader dropped while writer was active")
			}
			let new_count = old_count - 1;
			match self.0.compare_exchange_weak(old_count, new_count, Ordering::Release, Ordering::Acquire) {
				Ok(_) => return,
				Err(new_old_count) => old_count = new_old_count
			}
		}
	}
}

impl core::fmt::Debug for RwCount {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mut d = f.debug_struct("RwCount");
		let val = self.0.load(Ordering::Relaxed);
		let write = val & Self::WRITE_BIT_MASK != 0;
		let read = val & Self::READ_COUNT_MASK;
		d.field("write", &write);
		d.field("read", &read);
		d.finish()
	}
}

pub struct RwLock<T: ?Sized> {
	lock: RwCount,
	poisoned: AtomicBool,
	data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
	pub const fn new(val: T) -> Self {
		Self {
			lock: RwCount::new(),
			poisoned: AtomicBool::new(false),
			data: UnsafeCell::new(val)
		}
	}
}

impl<T: ?Sized> RwLock<T> {
	pub fn unpoison(&self) { self.poisoned.store(false, Ordering::Release); }

	unsafe fn poison(&self) {
		self.poisoned.store(true, Ordering::Release);
	}

	unsafe fn unlock_write(&self, flags: Flags) {
		self.lock.unlock_write();
		#[cfg(not(feature = "test"))] reset_interrupts(flags);
	}

	unsafe fn unlock_read(&self, flags: Flags) {
		self.lock.unlock_read();
		#[cfg(not(feature = "test"))] reset_interrupts(flags);
	}

	pub fn read(&self) -> LockResult<ReadGuard<'_, T>> {
		// Lock local cpu first to prevent potential deadlocks from an interrupt occurring after the multicore lock
		let flags: Flags;
		#[cfg(not(feature = "test"))] { flags = disable_interrupts(); }
		#[cfg(feature = "test")] { flags = Flags(0); }

		self.lock.spin_lock_read();

		let guard = ReadGuard {
			rwlock: self,
			flags
		};

		if self.poisoned.load(Ordering::Acquire) { Err(PoisonError::new(guard)) }
		else { Ok(guard) }
	}

	pub fn try_read(&self) -> TryLockResult<ReadGuard<'_, T>> {
		// Lock local cpu first to prevent potential deadlocks from an interrupt occurring after the multicore lock
		let flags: Flags;
		#[cfg(not(feature = "test"))] { flags = disable_interrupts(); }
		#[cfg(feature = "test")] { flags = Flags(0); }

		if self.lock.try_lock_read() {
			let guard = ReadGuard {
				rwlock: self,
				flags
			};

			if self.poisoned.load(Ordering::Acquire) { Err(TryLockError::Poisoned(PoisonError::new(guard))) }
			else { Ok(guard) }
		} else {
			// If couldn't acquire global lock, then unlock local lock too
			#[cfg(not(feature = "test"))] reset_interrupts(flags);

			Err(TryLockError::WouldSpin)
		}
	}

	pub fn write(&self) -> LockResult<WriteGuard<'_, T>> {
		// Lock local cpu first to prevent potential deadlocks from an interrupt occurring after the multicore lock
		let flags: Flags;
		#[cfg(not(feature = "test"))] { flags = disable_interrupts(); }
		#[cfg(feature = "test")] { flags = Flags(0); }

		self.lock.spin_lock_write();

		let guard = WriteGuard {
			rwlock: self,
			flags
		};

		if self.poisoned.load(Ordering::Acquire) { Err(PoisonError::new(guard)) }
		else { Ok(guard) }
	}

	pub fn try_write(&self) -> TryLockResult<WriteGuard<'_, T>> {
		// Lock local cpu first to prevent potential deadlocks from an interrupt occurring after the multicore lock
		let flags: Flags;
		#[cfg(not(feature = "test"))] { flags = disable_interrupts(); }
		#[cfg(feature = "test")] { flags = Flags(0); }

		if self.lock.try_lock_write() {
			let guard = WriteGuard {
				rwlock: self,
				flags
			};

			if self.poisoned.load(Ordering::Acquire) { Err(TryLockError::Poisoned(PoisonError::new(guard))) }
			else { Ok(guard) }
		} else {
			// If couldn't acquire global lock, then unlock local lock too
			#[cfg(not(feature = "test"))] reset_interrupts(flags);

			Err(TryLockError::WouldSpin)
		}
	}
}

impl<T: ?Sized + core::fmt::Debug> core::fmt::Debug for RwLock<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mut d = f.debug_struct("RwLock");
		match self.try_read() {
			Ok(guard) => {
				d.field("data", &&*guard);
			}
			Err(TryLockError::Poisoned(err)) => {
				d.field("data", &&**err.get_ref());
			}
			Err(TryLockError::WouldSpin) => {
				struct LockedPlaceholder;
				impl core::fmt::Debug for LockedPlaceholder { fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result { f.write_str("<locked>") } }
				d.field("data", &LockedPlaceholder);
			}
		}
		d.field("poisoned", &self.poisoned.load(Ordering::Relaxed));
		d.field("lock", &self.lock);
		d.finish()
	}
}

pub struct ReadGuard<'a, T: 'a + ?Sized> {
	rwlock: &'a RwLock<T>,
	flags: Flags
}

impl<'a, T: 'a + ?Sized> Drop for ReadGuard<'a, T> {
	fn drop(&mut self) {
		unsafe {
			self.rwlock.unlock_read(self.flags.clone());
		}
	}
}

impl<'a, T: 'a + ?Sized> Deref for ReadGuard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		unsafe { &*self.rwlock.data.get() }
	}
}

impl<'a, T: 'a + ?Sized + core::fmt::Debug> core::fmt::Debug for ReadGuard<'a, T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("RwReadGuard")
		 .field(&&**self)
		 .finish()
	}
}

pub struct WriteGuard<'a, T: 'a + ?Sized> {
	rwlock: &'a RwLock<T>,
	flags: Flags
}

impl<'a, T: 'a + ?Sized> Drop for WriteGuard<'a, T> {
	fn drop(&mut self) {
		unsafe {
			if crate::bridge::panicking() {
				self.rwlock.poison();
			}

			self.rwlock.unlock_write(self.flags.clone());
		}
	}
}

impl<'a, T: 'a + ?Sized> Deref for WriteGuard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		unsafe { &*self.rwlock.data.get() }
	}
}

impl<'a, T: 'a + ?Sized> DerefMut for WriteGuard<'a, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { &mut *self.rwlock.data.get() }
	}
}

impl<'a, T: 'a + ?Sized + core::fmt::Debug> core::fmt::Debug for WriteGuard<'a, T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("RwWriteGuard")
				.field(&&**self)
				.finish()
	}
}

impl<'a, T: 'a + ?Sized> !Send for ReadGuard<'a, T> {}
impl<'a, T: 'a + ?Sized> !Send for WriteGuard<'a, T> {}
unsafe impl<'a, T: 'a + ?Sized + Sync> Sync for ReadGuard<'a, T> {}
unsafe impl<'a, T: 'a + ?Sized + Sync> Sync for WriteGuard<'a, T> {}
