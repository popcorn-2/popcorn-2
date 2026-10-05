use core::sync::atomic::Ordering;
use kernel_api::sync::IrqCell;
use kernel_api::threading::ThreadState;
use crate::{arch, percpu};
use crate::memory::r#virtual::AddressSpaceExt;
use crate::task::collections::{PopResult, SinglyLinkedList};
use crate::task::{idle, OwnedTask, Task, TaskRefExt};

pub struct RoundRobin {
	run_queue: IrqCell<SinglyLinkedList>,
}

impl RoundRobin {
	pub const fn new() -> Self {
		Self {
			run_queue: IrqCell::new(SinglyLinkedList::new()),
		}
	}

	pub fn next_task(&self) -> Option<OwnedTask> {
		let mut guard = self.run_queue.lock();
		loop {
			match guard.pop_front() {
				PopResult::None => break None,
				PopResult::Some(task) => break Some(task),
				PopResult::Outdated(task) => {
					// task has been killed somewhere, but we are the owner => dealloc it
					Task::try_dealloc(task.0);
				}
			}
		}
	}

	pub fn enqueue(&self, task: OwnedTask) {
		let mut guard = self.run_queue.lock();
		guard.push_back(task);
	}
}

pub extern "C" fn scheduler_entry(return_frame: &mut arch::ReturnFrame) {
	// now just about to exit the kernel so nothing on our stack frame, therefore safe to
	// do a context switch
	percpu!(needs_reschedule).set(false);
	debug!("scheduler entered with {return_frame:#x?}");

	let previous = percpu!(current_task)
		.get()
		.and_then(|t| t.get_or_dead())
		.map(|(task, _)| task);

	if let Some(previous) = previous {
		return_frame.store_to(&previous.registers);

		match previous.state.compare_exchange(
			ThreadState::Running,
			ThreadState::Ready,
			Ordering::AcqRel,
			Ordering::Acquire,
		) {
			Ok(_) => {
				percpu!(scheduler).enqueue(OwnedTask(previous));
			}
			Err(ThreadState::Killed(code)) | Err(ThreadState::Zombie(code)) => {
				previous.state.store(ThreadState::Zombie(code), Ordering::Release);
				previous.try_dealloc();
			}
			Err(ThreadState::NearlyParked) => {
				match previous.state.compare_exchange(
					ThreadState::NearlyParked,
					ThreadState::Parked,
					Ordering::AcqRel,
					Ordering::Acquire,
				) {
					Ok(_) => {}
					Err(ThreadState::Ready) => {
						// Woken up while being parked
						percpu!(scheduler).enqueue(OwnedTask(previous));
					}
					Err(ThreadState::Killed(code)) | Err(ThreadState::Zombie(code)) => {
						previous.state.store(ThreadState::Zombie(code), Ordering::Release);
						previous.try_dealloc();
					}
					Err(_) => {}
				}
			}
			Err(ThreadState::Ready) => {
				// Woken up while being parked
				percpu!(scheduler).enqueue(OwnedTask(previous));
			}
			Err(_) => {}
		}
	}

	let next = loop {
		let next = percpu!(scheduler).next_task().unwrap_or_else(|| {
			percpu!(current_task).set(None);
			loop {
				idle::idle_entry();
				if let Some(task) = percpu!(scheduler).next_task() {
					break task;
				}
			}
		});

		match next.0.state.compare_exchange(
			ThreadState::Ready,
			ThreadState::Running,
			Ordering::AcqRel,
			Ordering::Acquire,
		) {
			Ok(_) => break next,
			Err(ThreadState::Killed(code)) | Err(ThreadState::Zombie(code)) => {
				next.0.state.store(ThreadState::Zombie(code), Ordering::Release);
				next.0.try_dealloc();
			}
			Err(_) => {
				// Task was not ready (e.g. Parked)
			}
		}
	};

	// SAFETY: AddressSpace owned by next task and cannot be deallocated while running
	unsafe {
		next.0.address_space.load();
	}
	return_frame.load_from(&next.0.registers);

	debug!("scheduler exited with {return_frame:#x?}");

	percpu!(current_task).set(Some(next.0.as_ref()));
	percpu!(needs_reschedule).set(false);
}
