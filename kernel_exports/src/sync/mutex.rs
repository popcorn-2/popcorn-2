use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::sync::{Flags, TryLockError};
use super::{LockResult, PoisonError, TryLockResult, reset_interrupts, disable_interrupts};

#[derive(Debug)]
pub struct Mutex<T: ?Sized> {
	#[cfg(feature = "smp")] locked: AtomicBool,
	poisoned: AtomicBool,
	data: UnsafeCell<T>,
}

impl<T> Mutex<T> {
	pub const fn new(val: T) -> Self {
		Self {
			#[cfg(feature = "smp")] locked: AtomicBool::new(false),
			poisoned: AtomicBool::new(false),
			data: UnsafeCell::new(val)
		}
	}
}

impl<T: ?Sized> Mutex<T> {
	pub fn unpoison(&self) { self.poisoned.store(false, Ordering::Release); }

	unsafe fn poison(&self) {
		self.poisoned.store(true, Ordering::Release);
	}

	unsafe fn unlock(&self, flags: Flags) {
		#[cfg(feature = "smp")] self.locked.store(false, Ordering::Release);
		#[cfg(not(feature = "test"))] reset_interrupts(flags);
	}

	pub fn lock(&self) -> LockResult<Guard<'_, T>> {
		// Lock local cpu first to prevent potential deadlocks from an interrupt occurring after the multicore lock
		let flags: Flags;
		#[cfg(not(feature = "test"))] { flags = disable_interrupts(); }
		#[cfg(feature = "test")] { flags = Flags(0); }

		#[cfg(feature = "smp")] while let Err(_) = self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Acquire) {}

		let guard = Guard {
			mutex: self,
			flags
		};

		if self.poisoned.load(Ordering::Acquire) { Err(PoisonError::new(guard)) }
		else { Ok(guard) }
	}

	pub fn try_lock(&self) -> TryLockResult<Guard<'_, T>> {
		// it can't spin without smp
		#[cfg(not(feature = "smp"))]
		match self.lock() {
			Ok(guard) => Ok(guard),
			Err(poison) => Err(TryLockError::Poisoned(poison))
		}

		#[cfg(feature = "smp")] {
			// Lock local cpu first to prevent potential deadlocks from an interrupt occurring after the multicore lock
			let flags: Flags;
			#[cfg(not(feature = "test"))] { flags = disable_interrupts(); }
			#[cfg(feature = "test")] { flags = Flags(0); }

			if self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Acquire).is_ok() {
				let guard = Guard {
					mutex: self,
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
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

pub struct Guard<'a, T: 'a + ?Sized> {
	mutex: &'a Mutex<T>,
	flags: Flags
}

impl<'a, T: 'a + ?Sized> Drop for Guard<'a, T> {
	fn drop(&mut self) {
		unsafe {
			if crate::bridge::panicking() {
				self.mutex.poison();
			}

			self.mutex.unlock(self.flags.clone());
		}
	}
}

impl<'a, T: 'a + ?Sized> Deref for Guard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		unsafe { &*self.mutex.data.get() }
	}
}

impl<'a, T: 'a + ?Sized> DerefMut for Guard<'a, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { &mut *self.mutex.data.get() }
	}
}

impl<'a, T: ?Sized> !Send for Guard<'a, T> {}
unsafe impl<'a, T: ?Sized + Sync + 'a> Sync for Guard<'a, T> {}
