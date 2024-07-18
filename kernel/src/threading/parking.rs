use alloc::sync::{Arc, Weak};
use core::intrinsics::transmute;
use core::mem;
use core::mem::MaybeUninit;
use core::num::NonZeroU16;
use log::{debug, warn};
use super::{current_thread, scheduler, ThreadId, yield_now, scheduler::Scheduler, ThreadState, PointerState};

/// Park the current thread until it is woken up
///
/// This will park the thread until a [`Waker`] for this park event wakes the thread.
/// The kernel guarantees that this function will not return unless either there is an error
/// when parking the thread, or the thread is woken.
///
/// [`Waker`]s are generated in `park()`, and passed to the closure passed to `park()`.
/// A [`Waker`] will only wake a thread if it was generated for the current park event. This means
/// that a [`Waker`] will only wake a thread once, even if [`wake()`](Waker::wake) is
/// repeatedly called.
///
/// # Atomicity
///
/// The entirety of a call to `park()` executes atomically - if the thread is woken by a valid [`Waker`]
/// before `park()` has finished executing, it will act like [`yield_now()`], and a thread will never get stuck
/// waiting for an event that has already occurred.
///
/// This means the following code is guaranteed to always make progress:
/// ```rust
/// use kernel::threading::{park, WakeReason};
///
/// park(|waker| waker.wake(WakeReason::Custom(non_zero!(1)))).unwrap();
/// ```
///
/// # Errors
///
/// # Examples
///
/// ```
/// # use kernel_api::sync::Spinlock;
/// use kernel::threading::{park, Waker, WakeReason};
///
/// static WAKER: Spinlock<Option<Waker> = Spinlock::new(None);
///
/// // Called periodically by a timer interrupt
/// pub fn periodic_timer_handler() {
///     if let Some(waker) = &mut *WAKER.lock() {
///         waker.wake(WakeReason::Timeout);
///     }
/// }
///
/// // This call will return the next time the timer interrupt goes off
/// park(|waker| *WAKER.lock() = Some(waker)).expect("failed to park thread");
///
/// // This call will never return, even if the timer interrupt goes off again, unless
/// // there was an error in `park`
/// park(|_| {}).expect("failed to park thread");
/// unreachable!();
/// ```
///
/// Multiple wakers can be created by calling [`Clone::clone()`] on the passed [`Waker`]
///
/// ```
/// use kernel::threading::{park, Waker, WakeReason};
/// use core::time::Duration;
///
/// fn wake_in(waker: Waker, delay: Duration) { /* ... */ }
///
/// park(|waker| {
///     wake_in(waker.clone(), Duration::from_secs(1));
///     wake_in(waker, Duration::from_secs(2));
/// }).unwrap();
pub fn park(f: impl FnOnce(Waker)) -> Result<WakeReason, ParkError> {
	let id = current_thread();
	debug!("Parking thread {:?}", id.unwrap());
	let weak_ptr = {
		let mut guard = scheduler::local_scheduler().lock();
		let thread = guard.current_thread().expect("Cannot park when not running a thread");
		let park_state = Arc::new(ParkState { thread_id: *thread.thread_id });
		let weak_ptr = Arc::downgrade(&park_state);
		*thread.state = ThreadState::Parked(park_state);
		weak_ptr
	};
	// Set the state to `Parked` before calling the closure, so if events are triggered
	// during the closure, the thread already appears parked and will get unparked before yielding
	// Also drop the scheduler lock so that waking doesn't cause a deadlock
	f(Waker { park_state: weak_ptr });
	Ok(
		yield_now().expect("State was set to `Parked` before yielding so must have a reason to wake")
	)
}

#[derive(Debug)]
pub(super) struct ParkState {
	thread_id: ThreadId,
}

#[derive(Debug, Clone)]
pub struct Waker {
	park_state: Weak<ParkState>,
}

impl Waker {
	pub fn wake(&self, reason: WakeReason) {
		if let Some(state) = self.park_state.upgrade() {
			// FIXME: race condition between upgrading and actually waking which could cause a spurious wakeup
			let tid = state.thread_id;
			let mut guard = super::TASK_LIST.lock();
			let Some(global_thread) = guard.get_mut(&tid) else {
				warn!("Bad thread id {tid:?}");
				return;
			};

			match global_thread.1 {
				PointerState::InScheduler => {
					todo!()
				},
				PointerState::GloballyParked(_) => {
					let PointerState::GloballyParked(mut ptr) = mem::replace(&mut global_thread.1, PointerState::InScheduler) else { unreachable!() };
					
					debug!("Enqueue thread {:?} from global parking lot with reason {reason:?}", ptr.tcb_mut().thread_id);
					*ptr.tcb_mut().state = ThreadState::JustUnparked(reason);
					scheduler::enqueue(ptr);
				},
			};
		}
	}
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub enum WakeReason {
	Timeout,
	Custom(NonZeroU16),
}

#[derive(Debug)]
#[non_exhaustive]
pub struct ParkError {}
