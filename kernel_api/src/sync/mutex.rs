use core::arch::asm;
use core::convert::Into;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// A mutual exclusion primitive useful for protecting shared data
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub type Mutex<T> = lock_api::Mutex<RawSpinlock, T>;
#[unstable(feature = "kernel_spinlocks", issue = "none")]
pub type Spinlock<T> = lock_api::Mutex<RawSpinlock, T>;

/// An RAII implementation of a “scoped lock” of a mutex. When this structure is dropped (falls out of scope), the lock will be unlocked.
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub type MutexGuard<'a, T> = lock_api::MutexGuard<'a, RawSpinlock, T>;
#[unstable(feature = "kernel_spinlocks", issue = "none")]
pub type SpinlockGuard<'a, T> = lock_api::MutexGuard<'a, RawSpinlock, T>;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum State {
    Unlocked,
    Locked,
}

impl State {
    const fn const_into_u8(self) -> u8 {
        match self {
            State::Unlocked => 0,
            State::Locked => 1,
        }
    }

    const fn const_from_u8(value: u8) -> Result<Self, ()> {
        match value {
            0 => Ok(State::Unlocked),
            1 => Ok(State::Locked),
            _ => Err(())
        }
    }
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
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

#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub struct RawSpinlock {
    state: AtomicU8,
    irq_state: AtomicUsize
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
unsafe impl lock_api::RawMutex for RawSpinlock {
    const INIT: Self = Self {
        state: AtomicU8::new(State::Unlocked.const_into_u8()),
        irq_state: AtomicUsize::new(0),
    };

    type GuardMarker = lock_api::GuardNoSend; // Interrupts are only disabled on the locking core so sending guard

    fn lock(&self) {
        let irq_state = unsafe { crate::bridge::hal::__popcorn_disable_irq() };

        while let Err(_) = self.state.compare_exchange_weak(
            State::Unlocked.into(),
            State::Locked.into(),
            Ordering::Acquire,
            Ordering::Relaxed
        ) {
            core::hint::spin_loop();
        }

        self.irq_state.store(irq_state, Ordering::Relaxed);
    }

    fn try_lock(&self) -> bool {
        let irq_state = unsafe { crate::bridge::hal::__popcorn_disable_irq() };
        let success = self.state.compare_exchange(
            State::Unlocked.into(),
            State::Locked.into(),
            Ordering::Acquire,
            Ordering::Relaxed
        ).is_ok();

        if !success { unsafe { crate::bridge::hal::__popcorn_set_irq(irq_state) } }
        else { self.irq_state.store(irq_state, Ordering::Relaxed) }

        success
    }

    unsafe fn unlock(&self) {
        let old_irq_state = self.irq_state.load(Ordering::Relaxed);
        let old_state = self.state.swap(State::Unlocked.into(), Ordering::Release);
        let old_state = State::try_from(old_state).expect("Mutex in undefined state");

        match old_state {
            State::Unlocked => unreachable!("Mutex was unlocked while unlocked"),
            State::Locked => unsafe { crate::bridge::hal::__popcorn_set_irq(old_irq_state) },
        }
    }
}

/*
fn enable_irq() {
    #[cfg(target_arch = "x86_64")]
    unsafe { asm!("sti", options(preserves_flags, nomem)); }

    // FIXME: these flags should be the same as when interrupts were disabled
    #[cfg(target_arch = "aarch64")]
    unsafe { asm!("msr DAIFSet, #0b1111"); }
}

/// Returns whether interrupts were enabled before disablement
fn disable_irq() -> bool {
    #[cfg(target_arch = "x86_64")]
    fn disable() -> bool {
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

    #[cfg(target_arch = "aarch64")]
    fn disable() -> bool {
        let daif: u64;
        unsafe {
            asm!("
			mrs {}, DAIF
			msr DAIFClr, #0b1111
		", out(reg) daif)
        }

        (daif & 0b1111) != 0
    }

    disable()
}*/
