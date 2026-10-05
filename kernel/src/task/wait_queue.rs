use core::marker::PhantomPinned;
use core::ops::ControlFlow;
use core::pin::Pin;
use core::sync::atomic::Ordering;
use kernel_api::sync::Spinlock;
use kernel_api::threading::ThreadState;
use crate::percpu;
use crate::task::collections::{DoublyLinkedList, PopResult};
use crate::task::{OwnedTask, Task, TaskRefExt};

#[derive(Debug)]
pub struct WaitQueue {
	queue: Spinlock<DoublyLinkedList>,
	// Tasks store a pointer to the queue they're waiting on (to unlink on kill)
	// so we need the queue to be pinned.
	_pin: PhantomPinned,
}

impl WaitQueue {
	/// Creates a new, empty wait queue.
	pub const fn new() -> Self {
		Self {
			queue: Spinlock::new(DoublyLinkedList::new()),
			_pin: PhantomPinned,
		}
	}

	/// Puts the current task to sleep on the given `queue` if `condition` returns [`ControlFlow::Continue`].
	///
	/// `condition` is called while the waitqueue is locked, and so synchronises with [`wake_n_with`](Self::wake_n_with).
	/// i.e., if the checked condition is modified before a call to [`wake_n_with`](Self::wake_n_with), `condition` will
	/// either observe the new value, or the thread will be woken up - it will never observe the old value and miss the wakeup.
	///
	/// Returns the return value of `condition`.
	pub fn wait_if<R>(self: Pin<&Self>, condition: impl FnOnce() -> ControlFlow<R>) -> ControlFlow<R> {
		let current = percpu!(current_task)
			.get()
			.and_then(|t| t.get())
			.expect("cannot wait without a current task");

		// begin transition into parked
		match current.state.compare_exchange(
			ThreadState::Running,
			ThreadState::NearlyParked,
			Ordering::AcqRel,
			Ordering::Acquire,
		) {
			Ok(_) => {},
			Err(ThreadState::Killed(_)) => {
				// current was asynchronously killed - reschedule since we shouldn't return to dead task
				percpu!(needs_reschedule).set(true);
				// fixme: is this the correct return value?
				//  the task itself should never observe the result,
				//  but it may mess up kernel assumptions.
				return ControlFlow::Continue(());
			}
			Err(state) => {
				// current task should only be Running (since it is)
				// or Killed (asynchronously, and waiting for removal from
				// a runqueue)
				panic!("unexpected task state when waiting: {state:?}");
			}
		}

		// lock wait queue then run condition check
		let mut guard = self.queue.lock();
		if let Some(ret) = condition().break_value() {
			// if condition not met, try to return to task, unless
			// asynchronously modified in the background
			if current.state.compare_exchange(
				ThreadState::NearlyParked,
				ThreadState::Running,
				Ordering::AcqRel,
				Ordering::Acquire,
			).is_err() {
				percpu!(needs_reschedule).set(true);
			}
			return ControlFlow::Break(ret);
		}

		// `self` is pinned so fine to store a reference here
		let queue_ptr = core::ptr::from_ref(self.get_ref());
		current.intrusive.blocked_on.store(queue_ptr.cast_mut(), Ordering::Release);
		guard.push_back(OwnedTask(current));

		percpu!(needs_reschedule).set(true);
		ControlFlow::Continue(())
	}

	/// Wakes up to `n` tasks currently waiting on this wait queue, running `on_wake` on each.
	///
	/// [`usize::MAX`] can be passed to wake all tasks.
	///
	/// Returns the number of tasks that were woken.
	pub fn wake_n_with(self: Pin<&Self>, n: usize, mut on_wake: impl FnMut(&'static Task)) -> usize {
		let mut woken = 0;
		let mut guard = self.queue.lock();
		while woken < n {
			match guard.pop_front() {
				PopResult::None => break, // no more tasks waiting
				PopResult::Outdated(task) => {
					// remove runqueue pointer so in clean state for cleanup
					task.0.intrusive.blocked_on.store(core::ptr::null_mut(), Ordering::Release);
				}
				PopResult::Some(task) => {
					task.0.intrusive.blocked_on.store(core::ptr::null_mut(), Ordering::Release);
					on_wake(task.0);

					// transition into ready state
					let result = task.0.state.fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
						matches!(state, ThreadState::Parked | ThreadState::NearlyParked).then_some(ThreadState::Ready)
					});

					if result.is_ok() { woken += 1; } // task successfully woken
					if matches!(result, Ok(ThreadState::Parked)) {
						// enqueue if was not on runqueue
						crate::task::enqueue(task);
					}
				}
			}
		}
		woken
	}

	pub fn remove_task(&self, task: &'static Task) {
		let mut guard = self.queue.lock();
		if task.intrusive.blocked_on.load(Ordering::Relaxed).cast_const() == core::ptr::from_ref(self) {
			guard.remove(task);
			task.intrusive.blocked_on.store(core::ptr::null_mut(), Ordering::Release);
		}
	}
}

impl Default for WaitQueue {
	fn default() -> Self {
		Self::new()
	}
}
