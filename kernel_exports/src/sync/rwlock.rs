use core::cell::UnsafeCell;
use core::fmt::Formatter;
use core::mem;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use lock_api::RawRwLock;

use crate::sync::{Flags, TryLockError};

use super::{disable_interrupts, LockResult, PoisonError, reset_interrupts, TryLockResult};

pub type RwLock<T> = lock_api::RwLock<RwCount, T>;
pub type RwSpinlock<T> = lock_api::RwLock<RwCount, T>;
pub type RwReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RwCount, T>;
pub type RwReadSpinlockGuard<'a, T> = lock_api::RwLockReadGuard<'a, RwCount, T>;
pub type RwUpgradableReadGuard<'a, T> = lock_api::RwLockUpgradableReadGuard<'a, RwCount, T>;
pub type RwUpgradableReadSpinlockGuard<'a, T> = lock_api::RwLockUpgradableReadGuard<'a, RwCount, T>;
pub type RwWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RwCount, T>;
pub type RwWriteSpinlockGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RwCount, T>;

pub struct RwCount(AtomicUsize);

// FIXME: Deadlocks due to interrupts
impl RwCount {
	const WRITE_BIT_MASK: usize = 1<<(mem::size_of::<usize>() * 8 - 1);
	const UPGRADEABLE_BIT_MASK: usize = 1<<(mem::size_of::<usize>() * 8 - 2);
	const READ_COUNT_MASK: usize = !(Self::WRITE_BIT_MASK | Self::UPGRADEABLE_BIT_MASK);
}

impl core::fmt::Debug for RwCount {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mut d = f.debug_struct("RwCount");
		let val = self.0.load(Ordering::Relaxed);
		let write = val & Self::WRITE_BIT_MASK != 0;
		let read = val & Self::READ_COUNT_MASK;
		let upgradeable_reader = val & Self::UPGRADEABLE_BIT_MASK != 0;
		d.field("write", &write);
		d.field("read", &(read + if upgradeable_reader { 1 } else { 0 }));
		d.finish()
	}
}

unsafe impl lock_api::RawRwLock for RwCount {
	const INIT: Self = Self(AtomicUsize::new(0));
	type GuardMarker = lock_api::GuardSend;

	fn lock_shared(&self) {
		while !self.try_lock_shared() {
			core::hint::spin_loop();
		}
	}

	fn try_lock_shared(&self) -> bool {
		let mut old_value = self.0.load(Ordering::Acquire);

		loop {
			let old_normal_count = old_value & Self::READ_COUNT_MASK;

			if old_normal_count == Self::READ_COUNT_MASK { panic!("Reader count overflowed") }
			if (old_value & Self::WRITE_BIT_MASK) != 0 { return false; }

			let new_value = (old_normal_count + 1) | (old_value & Self::UPGRADEABLE_BIT_MASK);

			match self.0.compare_exchange_weak(old_value, new_value, Ordering::Acquire, Ordering::Acquire) {
				Ok(_) => return true,
				Err(new_old_value) => old_value = new_old_value
			}
		}
	}

	unsafe fn unlock_shared(&self) {
		let mut old_value = self.0.load(Ordering::Acquire);
		loop {
			let old_normal_count = old_value & !Self::UPGRADEABLE_BIT_MASK;

			if cfg!(debug_assertions) && (old_value & Self::WRITE_BIT_MASK != 0) {
				panic!("BUG: RwLock reader dropped while writer was active")
			}
			let new_value = (old_normal_count - 1) | (old_value & Self::UPGRADEABLE_BIT_MASK);
			match self.0.compare_exchange_weak(old_value, new_value, Ordering::Release, Ordering::Acquire) {
				Ok(_) => return,
				Err(new_old_value) => old_value = new_old_value
			}
		}
	}

	fn lock_exclusive(&self) {
		while !self.try_lock_exclusive() {
			core::hint::spin_loop();
		}
	}

	fn try_lock_exclusive(&self) -> bool {
		self.0.compare_exchange_weak(0, Self::WRITE_BIT_MASK, Ordering::Acquire, Ordering::Acquire)
			.is_ok()
	}

	unsafe fn unlock_exclusive(&self) {
		if cfg!(debug_assertions) {
			self.0.compare_exchange(Self::WRITE_BIT_MASK, 0, Ordering::Release, Ordering::Acquire)
				.expect("BUG: RwLock writer dropped while readers were active");
		} else {
			self.0.store(0, Ordering::Release);
		}
	}
}

unsafe impl lock_api::RawRwLockDowngrade for RwCount {
	unsafe fn downgrade(&self) {
		if cfg!(debug_assertions) {
			self.0.compare_exchange(Self::WRITE_BIT_MASK, 1, Ordering::Release, Ordering::Acquire)
				.expect("BUG: RwLock writer downgraded while readers were active");
		} else {
			// No existing readers should exist therefore can unconditionally set read count to 1
			self.0.store(1, Ordering::Release);
		}
	}
}

unsafe impl lock_api::RawRwLockUpgrade for RwCount {
	fn lock_upgradable(&self) {
		while !self.try_lock_upgradable() {
			core::hint::spin_loop();
		}
	}

	fn try_lock_upgradable(&self) -> bool {
		let mut old_value = self.0.load(Ordering::Acquire);

		loop {
			if (old_value & Self::WRITE_BIT_MASK) != 0 { return false; }

			let new_value = old_value | Self::UPGRADEABLE_BIT_MASK;

			match self.0.compare_exchange_weak(old_value, new_value, Ordering::Acquire, Ordering::Acquire) {
				Ok(_) => return true,
				Err(new_old_value) => old_value = new_old_value
			}
		}
	}

	unsafe fn unlock_upgradable(&self) {
		let mut old_value = self.0.load(Ordering::Acquire);
		loop {
			if cfg!(debug_assertions) && (old_value & Self::WRITE_BIT_MASK != 0) {
				panic!("BUG: RwLock upgradable reader dropped while writer was active")
			}
			let new_value = old_value & !Self::UPGRADEABLE_BIT_MASK;
			match self.0.compare_exchange_weak(old_value, new_value, Ordering::Release, Ordering::Acquire) {
				Ok(_) => return,
				Err(new_old_value) => old_value = new_old_value
			}
		}
	}

	unsafe fn upgrade(&self) {
		while !self.try_upgrade() {
			core::hint::spin_loop();
		}
	}

	unsafe fn try_upgrade(&self) -> bool {
		self.0.compare_exchange_weak(Self::UPGRADEABLE_BIT_MASK, Self::WRITE_BIT_MASK, Ordering::Acquire, Ordering::Acquire)
			.is_ok()
	}
}

unsafe impl lock_api::RawRwLockUpgradeDowngrade for RwCount {
	unsafe fn downgrade_upgradable(&self) {
		let mut old_value = self.0.load(Ordering::Acquire);
		loop {
			if cfg!(debug_assertions) && (old_value & Self::WRITE_BIT_MASK != 0) {
				panic!("BUG: RwLock upgradable reader downgraded while writer was active")
			}

			let old_normal_count = old_value & Self::READ_COUNT_MASK;
			if old_normal_count == Self::READ_COUNT_MASK { panic!("Reader count overflowed") }

			let new_value = old_normal_count + 1;
			match self.0.compare_exchange_weak(old_value, new_value, Ordering::Release, Ordering::Acquire) {
				Ok(_) => return,
				Err(new_old_value) => old_value = new_old_value
			}
		}
	}

	unsafe fn downgrade_to_upgradable(&self) {
		if cfg!(debug_assertions) {
			self.0.compare_exchange(Self::WRITE_BIT_MASK, 1, Ordering::Release, Ordering::Acquire)
				.expect("BUG: RwLock writer downgraded while readers were active");
		} else {
			// No existing readers should exist therefore can unconditionally set upgradable bit
			self.0.store(Self::UPGRADEABLE_BIT_MASK, Ordering::Release);
		}
	}
}
