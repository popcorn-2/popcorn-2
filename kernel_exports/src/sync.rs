use core::arch::asm;
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
#[cfg(feature = "smp")] use core::sync::atomic::{AtomicBool, Ordering};
use cfg_if::cfg_if;

// todo: fix thing
pub type RwLock<T> = Lock<T>;

#[derive(Debug)]
pub struct Lock<T> {
	#[cfg(feature = "smp")] locked: AtomicBool,
	data: UnsafeCell<T>,
}

unsafe impl<T> Sync for Lock<T> {}

impl<T> Lock<T> {
	pub const fn new(val: T) -> Self {
		Self {
			#[cfg(feature = "smp")] locked: AtomicBool::new(false),
			data: UnsafeCell::new(val)
		}
	}

	pub fn lock(&self) -> LockGuard<T> {
		#[cfg(feature = "smp")] while let Err(_) = self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Acquire) {}

		let flags: u64;
		#[cfg(not(feature = "test"))] unsafe {
			asm!("
						pushf
						pop {}
						cli
					", out(reg) flags);
		};
		#[cfg(feature = "test")] { flags = 0; }

		LockGuard {
			#[cfg(feature = "smp")] lock: &self.locked,
			data: unsafe { &mut *self.data.get() },
			flags
		}
	}
}

pub struct LockGuard<'a, T> {
	#[cfg(feature = "smp")] lock: &'a AtomicBool,
	data: &'a mut T,
	flags: u64
}

impl<'a, T> Drop for LockGuard<'a, T> {
	fn drop(&mut self) {
		#[cfg(feature = "smp")] self.lock.store(false, Ordering::Release);
		#[cfg(not(feature = "test"))] unsafe {
			asm!("
				push {}
				popf
			", in(reg) self.flags);
		}
	}
}

impl<'a, T> Deref for LockGuard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.data
	}
}

impl<'a, T> DerefMut for LockGuard<'a, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.data
	}
}
