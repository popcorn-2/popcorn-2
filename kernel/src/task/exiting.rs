use core::ops::ControlFlow;
use core::sync::atomic::Ordering;
use kernel_api::syscall;
use kernel_api::threading::{TaskRef, ThreadState};
use crate::percpu;
use crate::task::TaskRefExt;

pub fn kill_task(task_ref: TaskRef, exit_code: i32) -> syscall::Result<()> {
	let Some(task) = task_ref.get() else {
		return Err(syscall::Error::DeadServer);
	};

	if task.intrusive.bump(task_ref.generation()).is_err() {
		return Err(syscall::Error::DeadServer);
	}

	let old_state = task.state.fetch_update(
		Ordering::AcqRel,
		Ordering::Acquire,
		|state| match state {
			ThreadState::Killed(_) | ThreadState::Zombie(_) => None,
			_ => Some(ThreadState::Killed(exit_code)),
		},
	);

	// wake any tasks that have joined on this task
	task.exit_queue().wake_n_with(usize::MAX, |task| task.registers.set_return_result(Ok(exit_code as isize)));

	match old_state {
		Ok(ThreadState::Running) => {
			let is_current_core = percpu!(current_task)
				.get()
				.is_some_and(|curr| curr.addr_eq(task_ref));

			if is_current_core {
				// If running on current core, reschedule
				percpu!(needs_reschedule).set(true);
			} else {
				// If not on current core, send IPI to stop it
				todo!("send reschedule IPI to target core");
			}
		}
		Ok(ThreadState::Ready) => {
			// On run queue, scheduler will deallocate it
		}
		Ok(ThreadState::Parked) => {
			// The task was parked and not executing on any CPU.
			// If it is in a wait queue, remove it
			let queue = task.intrusive.blocked_on.load(Ordering::Acquire);
			// SAFETY: `queue_ptr` was stored via `wait_with` from a valid `WaitQueue` reference.
			//  The `WaitQueue` remains valid while tasks are enqueued on it.
			if let Some(queue) = unsafe { queue.as_ref() } {
				queue.remove_task(task);
			}

			// Parked => not on run queue
			// Just removed from waitqueue => not on waitqueue
			// therefore can switch to zombie state and dealloc
			task.state.store(ThreadState::Zombie(exit_code), Ordering::Release);
			task.try_dealloc();
		}
		Ok(ThreadState::NearlyParked) => {
			// The task is in the process of parking, so may still be running
			// If it has already been added to a wait queue, remove it so it won't get woken

			let queue = task.intrusive.blocked_on.load(Ordering::Acquire);
			// SAFETY: `queue_ptr` was stored via `wait_with` from a valid `WaitQueue` reference.
			//  The `WaitQueue` remains valid while tasks are enqueued on it.
			if let Some(queue) = unsafe { queue.as_ref() } {
				queue.remove_task(task);
			}

			// reschedule the core that is running it
			let is_current_core = percpu!(current_task)
				.get()
				.is_some_and(|curr| curr.addr_eq(task_ref));

			if is_current_core {
				// If running on current core, reschedule
				percpu!(needs_reschedule).set(true);
			} else {
				// If not on current core, send IPI to stop it
				todo!("send reschedule IPI to target core");
			}
		}
		Ok(ThreadState::Killed(_)) | Ok(ThreadState::Zombie(_)) | Err(_) => {
			// task was already dead
			return Err(syscall::Error::DeadServer);
		}
	}

	Ok(())
}

pub fn detach_task(task_ref: TaskRef) {
	if let Some((task, _)) = task_ref.get_or_dead() {
		task.detached.store(true, Ordering::Release);
		task.try_dealloc();
	}
}

pub fn wait_task(target: TaskRef) -> syscall::Result<i32> {
	let current = percpu!(current_task)
		.get()
		.and_then(|t| t.get())
		.expect("cannot wait without a current task");

	if current.as_ref().addr_eq(target) {
		return Err(syscall::Error::FutureCompat);
	}

	let Some((target_task, is_dead)) = target.get_or_dead() else {
		return Err(syscall::Error::DeadServer);
	};

	// target already exited
	if is_dead {
		let state = target_task.state.load(Ordering::Acquire);
		return match state {
			ThreadState::Zombie(code) | ThreadState::Killed(code) => Ok(code),
			_ => unreachable!("dead generation but not dead task"),
		}
	}

	let waited = target_task.exit_queue().wait_if(|| {
		let still_valid = target.get_or_dead().is_some();
		let state = target_task.state.load(Ordering::Acquire);

		match state {
			ThreadState::Zombie(code) | ThreadState::Killed(code) => {
				if still_valid {
					ControlFlow::Break(Ok(code))
				} else {
					ControlFlow::Break(Err(syscall::Error::DeadServer))
				}
			}
			_ => {
				if still_valid {
					ControlFlow::Continue(())
				} else {
					ControlFlow::Break(Err(syscall::Error::DeadServer))
				}
			}
		}
	});

	// the default value should never be observed by the calling thread
	waited.break_value().unwrap_or(Ok(0))
}
