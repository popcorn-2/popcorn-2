use kernel_api::sync::IrqCell;
use crate::{arch, percpu};
use crate::task::collections::{PopResult, SinglyLinkedList};
use crate::task::{Task, OwnedTask};

pub struct RoundRobin {
	run_queue: IrqCell<SinglyLinkedList>,
}

impl RoundRobin {
	pub const fn new() -> Self {
		Self {
			run_queue: IrqCell::new(SinglyLinkedList::new()),
		}
	}

	pub fn schedule(&self) -> Option<OwnedTask> {
		let mut guard = self.run_queue.lock();
		loop {
			match guard.pop_front() {
				PopResult::None => break None,
				PopResult::Some(task) => break Some(task),
				PopResult::Outdated(task) => {
					// task has been killed somewhere, but we are the owner => dealloc it
					Task::dealloc(task);
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
	todo!("do scheduling")
}
