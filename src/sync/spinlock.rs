use core::ops::{Deref, DerefMut};
#[cfg(feature = "smp")] use core::sync::atomic::{AtomicBool, Ordering};

pub struct Spinlock<T> {
	#[cfg(feature = "smp")] locked: AtomicBool,
	data: T,
}

impl<T> Spinlock<T> {
	pub const fn new(data: T) -> Self<> {
		Self {
			#[cfg(feature = "smp")] locked: AtomicBool::new(false),
			data
		}
	}

	pub fn lock(&self) -> SpinlockGuard<T> {
		#[cfg(feature = "smp")] while let Err(_) = self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Acquire) {}
		// todo: irq enable/disable
		SpinlockGuard {
			mutex: unsafe { &mut *{self as *const _ as *mut _} }
		}
	}
}

pub struct SpinlockGuard<'a, T> {
	mutex: &'a mut Spinlock<T>
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
	fn drop(&mut self) {
		#[cfg(feature = "smp")] self.mutex.locked.store(false, Ordering::Release);
		// todo: irq enable/disable
	}
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.mutex.data
	}
}

impl<'a, T> DerefMut for SpinlockGuard<'a, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.mutex.data
	}
}
