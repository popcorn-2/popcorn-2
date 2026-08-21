use core::mem::ManuallyDrop;
use core::sync::atomic::Ordering;
use log::{debug, trace};
use kernel_api::threading::ThreadState;
use kernel_api::time::Instant;
use crate::hal::{self, IpiTarget};
use super::{scheduler, scheduler::Scheduler, ThreadControlBlock};

/// Adds a pending thread switch that will switch threads once all nested interrupts are handled.
///
/// This will immediately return regardless of the current thread's blocked state.
///
/// # Interrupt safety
/// This function **is** interrupt safe, and will immediately return
#[expect(unused)]
pub fn yield_defer() {
	hal::send_ipi(IpiTarget::SelfIpi).expect("Failed to yield");
}

/// Immediately invokes the scheduler to switch threads.
///
/// If the thread has be placed into a blocked state, this will not return until it is unblocked.
///
/// # Interrupt safety
/// This function is **not** interrupt safe, and will block any pending interrupts
pub fn yield_now() {
	yield_now_inner(false)
}

pub fn yield_now_inner(inside_park: bool) {
	#[inline]
	fn do_thread_switch(from: ThreadControlBlock, to: &ThreadControlBlock, inside_park: bool) {
		assert!(to.state.runnable());
		to.state.store(ThreadState::Running, Ordering::SeqCst);

		if inside_park {
			debug!("yield inside maybe_park");
			// finish parking if needed
			let _ = from.state.compare_exchange(ThreadState::NearlyParked, ThreadState::Parked, Ordering::SeqCst, Ordering::SeqCst);
		}

		// SAFETY: The AddressSpace is owned by the thread, and thread is always alive while running
		unsafe {
			to.address_space.load();
		}

		let mut from = ManuallyDrop::new(from);

		// From the CPU's perspective during a context switch, `from` is no longer the same `&mut ThreadControlBlock`
		// as the stack has been changed. Instead, we replace it with the `&mut ThreadControlBlock` that `switch_thread`
		// preserves across the function call and then pull it out of the old thread's stack
		//
		// since the `from` TCB still exists, it's stack can't yet have been dropped, so it's safe to read from that memory
		let from = unsafe { hal::switch_thread(&mut from, to) };

		unsafe {
			post_switch_cleanup(from);
		}
	}

	let scheduler = scheduler::local_scheduler();
	let mut scheduler = scheduler.lock();
	let mut current_thread = percpu_v2!(current_thread).write();

	if let Some(new_thread) = scheduler.get_next_thread() {
		// todo: deal with `ThreadState::Killed`
		let old_thread = core::mem::replace(&mut *current_thread, Some(new_thread)).expect("cannot enter `yield_now` from idle");
		let mut current_thread = ManuallyDrop::new(current_thread);
		let new_thread = current_thread.as_mut().expect("Just added `new_thread`");

		core::mem::forget(scheduler);
		do_thread_switch(old_thread, new_thread, inside_park)
	} else {
		{
			let old_thread_tcb = current_thread.as_ref().expect("cannot enter `yield_now` while idle");
			// todo: sort out NearlyParked
			if old_thread_tcb.state.runnable() {
				trace!("no context switch");
				return;
			}
		}

		// remove TCB from `current_thread` to mark as idle
		let old_thread_tcb = current_thread.take().expect("cannot enter `yield_now` while idle");
		// scheduler and current task shouldn't be accessed from interrupts so don't need to unlock them
		let idle_start_time = Instant::now();
		debug!("start idling");
		let new_thread = loop {
			hal::wait_for_interrupt();
			if let Some(new_thread) = scheduler.get_next_thread() { break Some(new_thread); }
			if old_thread_tcb.state.runnable() { break None };
		};
		let idle_time = idle_start_time.elapsed();
		debug!("idled for {idle_time:?}");

		let Some(new_thread) = new_thread else {
			trace!("original thread runnable");
			*current_thread = Some(old_thread_tcb);
			return;
		};

		let mut current_thread = ManuallyDrop::new(current_thread);
		let new_thread = current_thread.insert(new_thread);
		// forget the scheduler guard because we unlock it from the new thread
		core::mem::forget(scheduler);
		do_thread_switch(old_thread_tcb, new_thread, inside_park)
	}
}

pub unsafe extern "C" fn post_switch_cleanup(previous_thread: &mut ManuallyDrop<ThreadControlBlock>) {
	// this reference points to somewhere on the stack of `previous_thread`
	// the stack must be live since the `ThreadControlBlock` that owns it exists on it, effectively
	// creating a reference cycle until we `take()` the `ThreadControlBlock`
	let previous_thread = unsafe { ManuallyDrop::take(previous_thread) };

	let mut guard = unsafe { scheduler::local_scheduler().make_guard_unchecked() };
	let _ = unsafe { percpu_v2!(current_thread).make_write_guard_unchecked() };

	trace!("[b] switch from `{:?}` to current, old in state {:?}", previous_thread.thread_id, previous_thread.state);

	match previous_thread.state.compare_exchange(ThreadState::Running, ThreadState::Ready, Ordering::SeqCst, Ordering::SeqCst) {
		Err(ThreadState::Killed(_)) => {
			guard.on_thread_exit(previous_thread.thread_id);
			drop(previous_thread);
		},
		_ => guard.put_thread(previous_thread),
	}
}
