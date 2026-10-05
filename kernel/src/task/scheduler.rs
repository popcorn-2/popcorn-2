use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use kernel_api::ptr::TaggedNonNull;
use kernel_api::sync::IrqCell;
use kernel_api::syscall;
use kernel_api::threading::{TaskRef, ThreadState};
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

		let donated_from = percpu!(current_task_donor).take();

		match previous.state.compare_exchange(
			ThreadState::Running,
			ThreadState::Ready,
			Ordering::AcqRel,
			Ordering::Acquire,
		) {
			Ok(_) => {
				// re-enqueue whichever task came from our own runqueue
				percpu!(scheduler).enqueue(donated_from.unwrap_or(OwnedTask(previous)));
			}
			Err(ThreadState::Killed(code)) | Err(ThreadState::Zombie(code)) => {
				// enqueue the root of the donation chain so it will chase up to the dead task
				// and propagate the dead state one step up the chain
				if let Some(donated_from) = donated_from {
					percpu!(scheduler).enqueue(donated_from);
				} else {
					// our own task was killed, therefore safe to dealloc
					previous.state.store(ThreadState::Zombie(code), Ordering::Release);
					previous.try_dealloc();
				}
			}
			Err(ThreadState::NearlyParked) => {
				match previous.state.compare_exchange(
					ThreadState::NearlyParked,
					ThreadState::Parked,
					Ordering::AcqRel,
					Ordering::Acquire,
				) {
					Ok(_) => {
						if let Some(donated_from) = donated_from {
							percpu!(scheduler).enqueue(donated_from);
						}
						// don't enqueue `previous` if it's our own task as it's now parked
					}
					Err(ThreadState::Ready) => {
						// Woken up while being parked
						// re-enqueue whichever task came from our own runqueue
						percpu!(scheduler).enqueue(donated_from.unwrap_or(OwnedTask(previous)));
					}
					Err(ThreadState::Killed(code)) | Err(ThreadState::Zombie(code)) => {
						// see note on outer cmpxchg dead state
						if let Some(donated_from) = donated_from {
							percpu!(scheduler).enqueue(donated_from);
						} else {
							previous.state.store(ThreadState::Zombie(code), Ordering::Release);
							previous.try_dealloc();
						}
					}
					Err(_) => {}
				}
			}
			Err(ThreadState::Ready) => {
				// Woken up while being parked
				// re-enqueue whichever task came from our own runqueue
				percpu!(scheduler).enqueue(donated_from.unwrap_or(OwnedTask(previous)));
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

		let mut donated_to = next.0;
		loop {
			let linked_to = donated_to.linked_to.load(Ordering::Relaxed);
			let linked_to = NonNull::new(linked_to);

			if linked_to.is_none() {
				// reached end of chain
				break;
			}

			let task_ref = linked_to.map(|linked_to| unsafe {
				TaskRef::from_raw(TaggedNonNull::from_tagged_ptr(linked_to.cast()))
			});

			if let Some(task) = task_ref.get() {
				donated_to = task;
			} else {
				donated_to.registers.set_return_result(Err(syscall::Error::DeadServer));
				donated_to.linked_to.store(ptr::null_mut(), Ordering::Release);
				break;
			}
		}

		match donated_to.state.compare_exchange(
			ThreadState::Ready,
			ThreadState::Running,
			Ordering::AcqRel,
			Ordering::Acquire,
		) {
			Ok(_) => {
				if !ptr::addr_eq(donated_to, next.0) {
					// if timeslice was donated, place original into donor variable
					percpu!(current_task_donor).set(Some(next));
				}

				break donated_to;
			},
			Err(_) => {
				// place original back onto runqueue as server already running elsewhere, or parked, etc.
				// (yes this means we skip the original task if a server died between chasing
				// donations and trying to run it, but unwinding the donation chain is too complex)
				percpu!(scheduler).enqueue(next);
			}
		}
	};

	// SAFETY: AddressSpace owned by next task and cannot be deallocated while running
	unsafe {
		next.address_space.load();
	}
	return_frame.load_from(&next.registers);

	debug!("scheduler exited with {return_frame:#x?}");

	percpu!(current_task).set(Some(next.as_ref()));
	percpu!(needs_reschedule).set(false);
}
