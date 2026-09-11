use core::convert::Into;
use core::mem::ManuallyDrop;
use core::panic::Location;
use core::sync::atomic::{AtomicPtr, AtomicU8, AtomicUsize, Ordering};
use log::warn;

/// A mutual exclusion primitive useful for protecting shared data
pub type Spinlock<T: ?Sized> = lock_api::Mutex<RawSpinlock, T>;

/// An RAII implementation of a “scoped lock” of a mutex.
/// 
/// When this structure is dropped (falls out of scope), the lock will be unlocked.
pub type SpinlockGuard<'a, T: ?Sized> = lock_api::MutexGuard<'a, RawSpinlock, T>;

pub type MappedSpinlockGuard<'a, T: ?Sized> = lock_api::MappedMutexGuard<'a, RawSpinlock, T>;

/// Extension functions to [`SpinlockGuard`]
pub trait SpinlockGuardExt {
    /// Unlock the spinlock without enabling interrupts, regardless of whether interrupts were enabled
    /// before the spinlock was locked
    fn unlock_no_interrupts(this: Self);
}

impl<T> SpinlockGuardExt for SpinlockGuard<'_, T> {
    fn unlock_no_interrupts(this: Self) {
        let this = ManuallyDrop::new(this);
        unsafe {
            let spinlock = Self::mutex(&this).raw();
            spinlock.unlock_no_interrupts();
        }
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

pub struct RawSpinlock {
    state: AtomicU8,
    irq_state: AtomicUsize,
    #[cfg(debug_assertions)] location: AtomicPtr<Location<'static>>,
}

unsafe impl Send for RawSpinlock {}

unsafe impl Sync for RawSpinlock {}

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
        unsafe { location.as_ref::<'static>() }
    }
}

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

        let mut p = true;
        while self.state.compare_exchange_weak(
            State::Unlocked.into(),
            State::Locked.into(),
            Ordering::Acquire,
            Ordering::Relaxed
        ).is_err() {
            core::hint::spin_loop();
            #[cfg(debug_assertions)] if p {
                p = false;
                warn!("locked at {:?}", self.lock_location());
            }
        }

        self.irq_state.store(irq_state, Ordering::Relaxed);
        #[cfg(debug_assertions)] self.location.store(Location::caller() as *const _ as *mut _, Ordering::Relaxed);
    }

    fn try_lock(&self) -> bool {
        let irq_state = crate::bridge::irq::disable();
        let success = self.state.compare_exchange(
            State::Unlocked.into(),
            State::Locked.into(),
            Ordering::Acquire,
            Ordering::Relaxed
        ).is_ok();

        if !success { crate::bridge::irq::set(irq_state) }
        else { self.irq_state.store(irq_state, Ordering::Relaxed) }

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
