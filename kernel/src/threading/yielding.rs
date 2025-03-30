use core::mem::ManuallyDrop;
use core::ptr;
use core::ptr::addr_of;
use log::{debug, trace};
use crate::hal::{ContextSwitchPreserve, self, IpiTarget};
use super::{scheduler, WakeReason, ThreadState, scheduler::Scheduler};

/// Adds a pending thread switch that will switch threads once all nested interrupts are handled.
///
/// This will immediately return regardless of the current thread's blocked state.
///
/// # Interrupt safety
/// This function **is** interrupt safe, and will immediately return
pub fn yield_defer() {
	hal::send_ipi(IpiTarget::SelfIpi).expect("Failed to yield");
}

/// Immediately invokes the scheduler to switch threads.
///
/// If the thread has be placed into a blocked state, this will not return until it is unblocked.
///
/// # Interrupt safety
/// This function is **not** interrupt safe, and will block any pending interrupts
pub fn yield_now() -> Option<WakeReason> {
	// First we lock the scheduler for the current core, and ask it for the current and new threads
	// Wrap it in `ManuallyDrop` since we recreate the guard later, as the thread may have migrated
	// during the context switch
	let mut scheduler = ManuallyDrop::new(scheduler::local_scheduler().lock());
	let (from, to_view) = scheduler.switch_thread_pre();

	// We need to duplicate the `ThreadPointer` so we can pass it to `switch_thread` while it is borrowed
	// so wrap the first copy in a `ManuallyDrop` to prevent a double free
	let mut from = ManuallyDrop::new(from);
	// Get a pointer to the `ThreadPointer` to use to duplicate it later
	let from_ptr = addr_of!(*from);
	// Extract the `PointerView`
	// The `PointerView` does not borrow the contents of the `ThreadPointer` - it only requires the
	// `ThreadPointer` to exist 'somewhere', so holding this borrow while moving the underlying `ThreadPointer`
	// is safe
	let from_view = from.tcb_mut();

	#[cfg(feature = "log.scheduler")] trace!("[a] switch from `{:?}` to `{:?}`", from_view.thread_id, to_view.thread_id);

	assert!(to_view.state.is_ready());
	let reason = to_view.state.wake_reason();
	*to_view.state = ThreadState::Running;
	if from_view.state.is_running() { *from_view.state = ThreadState::Ready; }

	// From the CPU's perspective during a context switch, `from` is no longer the same `ThreadPointer`
	// as the stack has been changed. Instead, we replace it with the `ThreadPointer` that `switch_thread`
	// preserves across the function call
	let ContextSwitchPreserve(mut from, reason) = unsafe { hal::switch_thread(&from_view, &to_view, ContextSwitchPreserve(ptr::read(from_ptr), reason)) };

	{
		let tcb = from.tcb_mut();
		#[cfg(feature = "log.scheduler")] trace!("[b] switch from `{:?}` to current, old blocked in state {:?}, new woken due to {reason:?}", tcb.thread_id, tcb.state);
	}

	// Then we pass the old `ThreadPointer` back to the scheduler for it to enqueue
	unsafe { scheduler::local_scheduler().make_guard_unchecked() }.switch_thread_post(from);

	reason
}
