use core::arch::asm;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
#[cfg(feature = "smp")] use core::sync::atomic::{AtomicBool, Ordering};

pub struct Spinlock<T> {
	#[cfg(feature = "smp")] locked: AtomicBool,
	data: UnsafeCell<T>,
}

impl<T> Spinlock<T> {
	pub const fn new(data: T) -> Self<> {
		Self {
			#[cfg(feature = "smp")] locked: AtomicBool::new(false),
			data: UnsafeCell::new(data)
		}
	}

	pub fn lock(&self) -> Guard<T> {
		#[cfg(feature = "smp")] while let Err(_) = self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Acquire) {}
		// todo: irq enable/disable
		let flags: u64;
		unsafe {
			asm!("
				pushf
				pop {}
				cli
			", out(reg) flags);
		}
		Guard {
			#[cfg(feature = "smp")] lock: &self.locked,
			data: unsafe { &mut *self.data.get() },
			flags
		}
	}
}

unsafe impl<T> Sync for Spinlock<T> {}

pub struct Guard<'a, T> {
	#[cfg(feature = "smp")] lock: &'a AtomicBool,
	data: &'a mut T,
	flags: u64
}

impl<'a, T> Drop for Guard<'a, T> {
	fn drop(&mut self) {
		#[cfg(feature = "smp")] self.lock.store(false, Ordering::Release);
		unsafe {
			asm!("
				push {}
				popf
			", in(reg) self.flags);
		}
	}
}

impl<'a, T> Deref for Guard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		self.data
	}
}

impl<'a, T> DerefMut for Guard<'a, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		self.data
	}
}
