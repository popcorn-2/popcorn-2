use crate::prelude::*;
use core::cell::{LazyCell, UnsafeCell};
use core::mem::ManuallyDrop;
use core::ptr;
use core::ptr::addr_of;
use core::sync::atomic::Ordering;
use log::{debug, trace};
use kernel_api::memory::physical::highmem;
use kernel_api::sync::{IrqCell, IrqGuard};
use crate::hal::{ContextSwitchPreserve, self, IpiTarget, TTableTy};
use crate::hal::paging2::TTable;
use crate::ipc::handle::HandleMap;
use crate::memory::paging::ktable;
use crate::memory::r#virtual::AddressSpaceInner;
use super::{scheduler, WakeReason, ThreadState, scheduler::Scheduler, ThreadPointer, PointerView, Thread, ThreadControlBlock, ThreadId, ControlEvent};

pub fn create_idle_thread() -> (ThreadId, Thread, UnsafeCell<ThreadPointer>) {
	extern "C" fn idle_loop(_: usize) -> ! {
		loop {
			hal::wait_for_interrupt();
			yield_now();
		}
	}

	let address_space = AddressSpaceInner::empty().expect("Could not create idle thread");

	let (tcb, id) = ThreadControlBlock::new(
		"<idle>".into(),
		address_space,
		HandleMap::new(),
		crate::threading::thread_startup,
		idle_loop,
		0,
	);

	let (thread, ptr) = ThreadPointer::new(Thread::new(tcb));

	debug!("Create idle thread with {id:?}");

	(id, thread, UnsafeCell::new(ptr))
}

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
	#[inline]
	fn do_thread_switch(from: ThreadPointer, mut to_view: PointerView) -> Option<WakeReason> {
		// We need to duplicate the `ThreadPointer` so we can pass it to `switch_thread` while it is borrowed
		// so wrap the first copy in a `ManuallyDrop` to prevent a double free
		let mut from = ManuallyDrop::new(from);
		// Get a pointer to the `ThreadPointer` to use to duplicate it later
		let from_ptr = addr_of!(*from);
		// Extract the `PointerView`
		// The `PointerView` does not borrow the contents of the `ThreadPointer` - it only requires the
		// `ThreadPointer` to exist 'somewhere', so holding this borrow while moving the underlying `ThreadPointer`
		// is safe
		let mut from_view = from.tcb_mut();

		#[cfg(feature = "log.scheduler")] trace!("[a] switch from `{:?}` to `{:?}`", from_view.thread_id, to_view.thread_id);

		let to_state = to_view.state.load(Ordering::SeqCst);
		assert!(to_state.is_ready());
		let reason = to_state.wake_reason();
		to_view.state.store(ThreadState::Running, Ordering::SeqCst);
		from_view.state.compare_exchange(ThreadState::Running, ThreadState::Ready, Ordering::SeqCst, Ordering::SeqCst);

		// SAFETY: The AddressSpace is owned by the thread, and thread is always alive while running
		unsafe {
			AddressSpaceInner::to_api(to_view.address_space).load();
		}

		// From the CPU's perspective during a context switch, `from` is no longer the same `ThreadPointer`
		// as the stack has been changed. Instead, we replace it with the `ThreadPointer` that `switch_thread`
		// preserves across the function call
		let ContextSwitchPreserve(from, reason) = unsafe { hal::switch_thread(&mut from_view, &mut to_view, ContextSwitchPreserve(ptr::read(from_ptr), reason)) };

		post_switch_cleanup(from);
		
		#[cfg(feature = "log.scheduler")] trace!("new woken due to {reason:?}");
		
		reason
	}
	
	// First we lock the scheduler for the current core, and ask it for the current and new threads
	// Wrap it in `ManuallyDrop` since we recreate the guard later, as the thread may have migrated
	// during the context switch
	let (scheduler, queue) = scheduler::local_scheduler();
	let mut scheduler = scheduler.lock();

	while let Some(event) = queue.pop() {
		match event {
			ControlEvent::Unpark(id, reason) => {
				match scheduler.unpark(id, reason) {
					Ok(_) => {},
					Err(_) => super::parking::do_wake(id, reason),
				}
			},
			ControlEvent::Kill(id) => if Some(id) == percpu_v2!(current_thread).read().as_ref().map(|tcb| *tcb.tcb_ref().thread_id) {
				super::exit(i8::MIN);
			} else {
				match scheduler.kill(id) {
					Ok(_) => {},
					Err(_) => super::kill(id),
				}
			}
		}
	}
	
	let mut scheduler = ManuallyDrop::new(scheduler);
	
	if let Some(new_thread) = scheduler.get_next_thread() {
		let mut guard = ManuallyDrop::new(percpu_v2!(current_thread).write());
		let old_thread = core::mem::replace(&mut **guard, Some(new_thread));
		let new_thread = guard.as_mut().expect("Just added `new_thread`");
		
		match old_thread {
			Some(old_thread) => do_thread_switch(old_thread, new_thread.tcb_mut()),
			None => {
				// no old thread, so switching from the idle thread
				debug!("stopped idling");

				/*
					SAFETY:
					The idle `Thread` object is never dropped, so the `ThreadPointer` is always valid
					The `ThreadPointer` is duplicated here (which is valid as internally they are raw pointers)
					and only once instance (passed into `do_thread_switch()`) has mutable references materialised from it.
					In `post_switch_cleanup()` if the previous thread is the idle thread (as would be the case
					here), the `ThreadPointer` is dropped and so we are back to only once copy
				 */
				do_thread_switch(unsafe { ptr::read(percpu_v2!(idle_thread).2.get()) }, new_thread.tcb_mut())
			}
		}
	} else {
		let mut guard = ManuallyDrop::new(percpu_v2!(current_thread).write());
		// no new thread, so either keep running old thread, or idle
		let old_thread = guard.take();
		
		match old_thread {
			Some(mut old_thread) => if old_thread.tcb_ref().state.load(Ordering::SeqCst).is_running() || old_thread.tcb_ref().state.load(Ordering::Relaxed).is_ready() {
				debug!("no context switch");
				
				// No changes occur to scheduler so just return back to thread, but make sure scheduler gets unlocked
				// as well since that would normally be done by the thread switch
				ManuallyDrop::into_inner(scheduler);
				let mut guard = ManuallyDrop::into_inner(guard);
				
				let old_thread = guard.insert(old_thread);
				
				match old_thread.tcb_ref().state.load(Ordering::SeqCst) {
					ThreadState::JustUnparked(reason) => Some(reason),
					_ => None,
				}
			} else {
				// old thread not runnable and no new thread, start idling
				debug!("start idling");

				/*
				SAFETY:
				The idle `Thread` object is never dropped, so the `PointerView` is always valid
				Cannot have more than one `PointerView` alive as it is only materialised here, and immediately dropped
				Reentrancy cannot occur here as interrupts are disabled due to the scheduler lock
			    */
				do_thread_switch(old_thread, unsafe { &mut *percpu_v2!(idle_thread).2.get() }.tcb_mut())
			},
			None => {
				debug!("continue idle");
				// just keep running idle thread - see notes on return to old thread
				ManuallyDrop::into_inner(scheduler);
				let _ = ManuallyDrop::into_inner(guard);
				None
			}
		}
	}
}

pub extern "C" fn post_switch_cleanup(mut previous_thread: ThreadPointer) {
	let mut guard = unsafe { scheduler::local_scheduler().0.make_guard_unchecked() };
	let _ = unsafe { percpu_v2!(current_thread).force_unlock_write() };
	let tcb = previous_thread.tcb_mut();
	trace!("[b] switch from `{:?}` to current, old blocked in state {:?}", tcb.thread_id, tcb.state);
	if *tcb.thread_id != percpu_v2!(idle_thread).0 {
		// Don't enqueue idle thread into a scheduler
		guard.switch_thread_post(previous_thread);
	} else {
		#[cfg(feature = "log.scheduler")] trace!("Ignoring enqueue of idle thread");
	}
}
