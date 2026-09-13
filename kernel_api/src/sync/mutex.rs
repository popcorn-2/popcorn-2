use core::convert::Into;
use core::mem::ManuallyDrop;
use core::panic::Location;
use core::sync::atomic::{AtomicPtr, AtomicU8, AtomicUsize, Ordering};
use log::warn;

/// A mutual exclusion primitive useful for protecting shared data.
///
/// This spinlock will block threads waiting for the lock to become available. The
/// spinlock can also be statically initialized or created via a `new`
/// constructor. Each spinlock has a type parameter which represents the data that
/// it is protecting. The data can only be accessed through the RAII guards
/// returned from `lock` and `try_lock`, which guarantees that the data is only
/// ever accessed when the spinlock is locked.
pub type Spinlock<T> = lock_api::Mutex<RawSpinlock, T>;

/// An RAII implementation of a "scoped lock" of a spinlock. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the spinlock can be accessed through this guard via its
/// `Deref` and `DerefMut` implementations.
pub type SpinlockGuard<'a, T> = lock_api::MutexGuard<'a, RawSpinlock, T>;

/// An RAII spinlock guard returned by `SpinlockGuard::map`, which can point to a
/// subfield of the protected data.
pub type MappedSpinlockGuard<'a, T> = lock_api::MappedMutexGuard<'a, RawSpinlock, T>;

/// Extension functions to [`SpinlockGuard`].
pub trait SpinlockGuardExt {
    /// Unlock the spinlock without enabling interrupts, regardless of whether interrupts were enabled
    /// before the spinlock was locked.
    fn unlock_no_interrupts(this: Self);
}

impl<T> SpinlockGuardExt for SpinlockGuard<'_, T> {
    fn unlock_no_interrupts(this: Self) {
        let this = ManuallyDrop::new(this);
	    // SAFETY: We consume the guard, preventing a guard from existing after we unlock the mutex
	    let spinlock = unsafe { Self::mutex(&this).raw() };
	    // SAFETY: `self` consuming method on `SpinlockGuard` so this thread must own a guard
	    unsafe { spinlock.unlock_no_interrupts() };
    }
}

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

// INVARIANT: `location` is always a valid 'static ref to a `Location<'static>` unless null
#[derive(Debug)]
#[doc(hidden)]
pub struct RawSpinlock {
    state: AtomicU8,
    irq_state: AtomicUsize,
    #[cfg(debug_assertions)] location: AtomicPtr<Location<'static>>,
}

impl RawSpinlock {
	/// # Safety
	///
	/// This method may only be called if the mutex is held in the current context, i.e. it must
	/// be paired with a successful call to [`lock`](`Self::lock`) or [`try_lock`](`Self::try_lock`).
    unsafe fn unlock_no_interrupts(&self) {
        let old_state = self.state.swap(State::Unlocked.into(), Ordering::Release);
        let old_state = State::try_from(old_state).expect("Spinlock in undefined state");

        match old_state {
            State::Unlocked => unreachable!("Mutex was unlocked while unlocked"),
            State::Locked => {},
        }
    }

    #[cfg(debug_assertions)]
    fn lock_location(&self) -> Option<&'static Location<'static>> {
        let location = self.location.load(Ordering::Relaxed);
	    // SAFETY: invariant of type that `location` is always valid unless null
        unsafe { location.as_ref::<'static>() }
    }
}

// SAFETY: locked state is only modified in `lock` and `unlock` functions which both check the existing state
unsafe impl lock_api::RawMutex for RawSpinlock {
    const INIT: Self = Self {
        state: AtomicU8::new(State::Unlocked.const_into_u8()),
        irq_state: AtomicUsize::new(0),
        #[cfg(debug_assertions)] location: AtomicPtr::new(core::ptr::null_mut()),
    };

    type GuardMarker = lock_api::GuardNoSend; // Dropping guard on other core would cause interrupts to be enabled in the wrong place

    #[track_caller]
    fn lock(&self) {
        let irq_state = crate::bridge::irq::disable();

        let mut printed = true;
        while self.state.compare_exchange_weak(
            State::Unlocked.into(),
            State::Locked.into(),
            Ordering::Acquire,
            Ordering::Relaxed
        ).is_err() {
            core::hint::spin_loop();
            if printed {
                printed = false;
                warn!("locked at {:?}", self.lock_location());
            }
        }

        self.irq_state.store(irq_state, Ordering::Relaxed);
        #[cfg(debug_assertions)] self.location.store(core::ptr::from_ref(Location::caller()).cast_mut(), Ordering::Relaxed);
    }

    fn try_lock(&self) -> bool {
        let irq_state = crate::bridge::irq::disable();
        let success = self.state.compare_exchange(
            State::Unlocked.into(),
            State::Locked.into(),
            Ordering::Acquire,
            Ordering::Relaxed
        ).is_ok();

        if success { self.irq_state.store(irq_state, Ordering::Relaxed) }
        else { crate::bridge::irq::set(irq_state) }

        success
    }

    unsafe fn unlock(&self) {
        let old_irq_state = self.irq_state.load(Ordering::Relaxed);
        let old_state = self.state.swap(State::Unlocked.into(), Ordering::Release);
        let old_state = State::try_from(old_state).expect("Spinlock in undefined state");

        match old_state {
            State::Unlocked => unreachable!("Mutex was unlocked while unlocked"),
            State::Locked => crate::bridge::irq::set(old_irq_state),
        }
    }
}
