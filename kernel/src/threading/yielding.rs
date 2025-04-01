use core::cell::{LazyCell, UnsafeCell};
use core::mem::ManuallyDrop;
use core::ptr;
use core::ptr::addr_of;
use log::{debug, trace};
use kernel_api::memory::physical::highmem;
use kernel_api::sync::{IrqCell, IrqGuard};
use crate::hal::{ContextSwitchPreserve, self, IpiTarget, TTableTy};
use crate::hal::paging2::TTable;
use crate::memory::paging::ktable;
use crate::threading::scheduler::SchedulerSwitchState;
use super::{scheduler, WakeReason, ThreadState, scheduler::Scheduler, ThreadPointer, PointerView, Thread, ThreadControlBlock, ThreadId};

#[thread_local]
static IDLE_THREAD: LazyCell<(ThreadId, Thread, UnsafeCell<ThreadPointer>)> = LazyCell::new(|| {
	extern "C" fn idle_loop(_: usize) -> ! {
		loop {
			hal::wait_for_interrupt();
			yield_now();
		}
	}

	let ttable = TTableTy::new(&*ktable(), highmem()).unwrap();
	let (tcb, id) = ThreadControlBlock::new(
		"<idle>".into(),
		ttable,
		crate::threading::thread_startup,
		idle_loop,
		0,
	);

	let (thread, ptr) = ThreadPointer::new(Thread::new(tcb));
	
	debug!("Create idle thread with {id:?}");

	(id, thread, UnsafeCell::new(ptr))
});

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
	fn do_thread_switch(from: ThreadPointer, to_view: PointerView) -> Option<WakeReason> {
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
		let ContextSwitchPreserve(from, reason) = unsafe { hal::switch_thread(&from_view, &to_view, ContextSwitchPreserve(ptr::read(from_ptr), reason)) };

		post_switch_cleanup(from);
		
		#[cfg(feature = "log.scheduler")] trace!("new woken due to {reason:?}");
		
		reason
	}
	
	// First we lock the scheduler for the current core, and ask it for the current and new threads
	// Wrap it in `ManuallyDrop` since we recreate the guard later, as the thread may have migrated
	// during the context switch
	let mut scheduler = ManuallyDrop::new(scheduler::local_scheduler().lock());
	
	match scheduler.switch_thread_pre() {
		SchedulerSwitchState::Switch { old_thread, new_thread } => do_thread_switch(old_thread, new_thread),
		SchedulerSwitchState::NoSwitch => {
			// No changes occur to scheduler so just return back to thread, but make sure scheduler gets unlocked
			// as well since that would normally be done by the thread switch
			let mut scheduler = ManuallyDrop::into_inner(scheduler);
			match scheduler.current_thread().expect("must be running a thread").state {
				ThreadState::JustUnparked(reason) => Some(*reason),
				_ => None,
			}
		},
		SchedulerSwitchState::Idle { old_thread } => {
			// Switch to the idle task
			
			/*
				SAFETY:
				The idle `Thread` object is never dropped, so the `PointerView` is always valid
				Cannot have more than one `PointerView` alive as it is only materialised here, and immediately dropped
				Reentrancy cannot occur here as interrupts are disabled due to the scheduler lock
			 */
			do_thread_switch(old_thread, unsafe { &mut *IDLE_THREAD.2.get() }.tcb_mut())
		},
		SchedulerSwitchState::SwitchFromIdle { new_thread } => {
			// Switch from the idle task
			
			/*
				SAFETY:
				The idle `Thread` object is never dropped, so the `ThreadPointer` is always valid
				The `ThreadPointer` is duplicated here (which is valid as internally they are raw pointers)
				and only once instance (passed into `do_thread_switch()`) has mutable references materialised from it.
				In `post_switch_cleanup()` if the previous thread is the idle thread (as would be the case
				here), the `ThreadPointer` is dropped and so we are back to only once copy
			 */
			do_thread_switch(unsafe { ptr::read(IDLE_THREAD.2.get()) }, new_thread)
		}
	}
}

pub extern "C" fn post_switch_cleanup(mut previous_thread: ThreadPointer) {
	let mut guard = unsafe { scheduler::local_scheduler().make_guard_unchecked() };
	let tcb = previous_thread.tcb_mut();
	trace!("[b] switch from `{:?}` to current, old blocked in state {:?}", tcb.thread_id, tcb.state);
	if *tcb.thread_id != IDLE_THREAD.0 {
		// Don't enqueue idle thread into a scheduler
		guard.switch_thread_post(previous_thread);
	} else {
		#[cfg(feature = "log.scheduler")] trace!("Ignoring enqueue of idle thread");
	}
}
