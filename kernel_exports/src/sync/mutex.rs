use core::arch::asm;
use core::convert::Into;
use core::sync::atomic::{AtomicU8, Ordering};

pub type Mutex<T> = lock_api::Mutex<RawSpinlock, T>;
pub type Spinlock<T> = lock_api::Mutex<RawSpinlock, T>;
pub type MutexGuard<'a, T> = lock_api::MutexGuard<'a, RawSpinlock, T>;
pub type SpinlockGuard<'a, T> = lock_api::MutexGuard<'a, RawSpinlock, T>;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
enum State {
	Unlocked = 0,
	LockedReenableIrq = 1,
	LockedNoIrq = 2
}

impl State {
	const fn const_into_u8(self) -> u8 {
		match self {
			State::Unlocked => 0,
			State::LockedReenableIrq => 1,
			State::LockedNoIrq => 2
		}
	}

	const fn const_from_u8(value: u8) -> Result<Self, ()> {
		match value {
			0 => Ok(State::Unlocked),
			1 => Ok(State::LockedReenableIrq),
			2 => Ok(State::LockedNoIrq),
			_ => Err(())
		}
	}


}

impl From<State> for u8 {
	fn from(value: State) -> Self {
		value.const_into_u8()
	}
}

impl TryFrom<u8> for State {
	type Error = ();

	fn try_from(value: u8) -> Result<Self, Self::Error> {
		Self::const_from_u8(value)
	}
}

pub struct RawSpinlock {
	state: AtomicU8
}

unsafe impl lock_api::RawMutex for RawSpinlock {
	const INIT: Self = Self {
		state: AtomicU8::new(State::Unlocked.const_into_u8())
	};

	type GuardMarker = lock_api::GuardNoSend; // Interrupts are only disabled on the locking core so sending guard

	fn lock(&self) {
		let irqs = disable_irq();

		while let Err(_) = self.state.compare_exchange(
			State::Unlocked.into(),
			if irqs { State::LockedReenableIrq.into() } else { State::LockedNoIrq.into() },
			Ordering::Acquire,
			Ordering::Acquire
		) {
			core::hint::spin_loop();
		}
	}

	fn try_lock(&self) -> bool {
		let irqs = disable_irq();
		let success = self.state.compare_exchange(
			State::Unlocked.into(),
			if irqs { State::LockedReenableIrq.into() } else { State::LockedNoIrq.into() },
			Ordering::Acquire,
			Ordering::Acquire
		).is_ok();

		if !success && irqs { enable_irq(); }

		success
	}

	unsafe fn unlock(&self) {
		let old_state = self.state.swap(State::Unlocked.into(), Ordering::Release);
		let old_state = State::try_from(old_state).expect("Mutex in undefined state");

		match old_state {
			State::Unlocked => unreachable!("Mutex was unlocked while unlocked"),
			State::LockedReenableIrq => enable_irq(),
			State::LockedNoIrq => {}
		}
	}
}

fn enable_irq() {
	unsafe { asm!("sti", options(preserves_flags, nomem)); }
}

/// Returns whether interrupts were enabled before disablement
fn disable_irq() -> bool {
	let flags: u64;
	unsafe {
		asm!("
			pushf
			pop {}
			cli
		", out(reg) flags, options(preserves_flags, nomem))
	}

	(flags & 0x0200) != 0
}
