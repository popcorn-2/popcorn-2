#[allow(unused_imports)] use crate::prelude::*;
use alloc::collections::VecDeque;
use core::fmt::Debug;
use kernel_api::time::Instant;
use crate::threading::scheduler::SchedulerMut;
use crate::threading::{ThreadId, ThreadPointer};
use event::{Queue, Event};
use crate::threading::tcb::PointerView;

mod event;

#[derive(Debug)]
pub struct TicklessRoundRobin {
	run_queue: VecDeque<ThreadPointer>,
	current_thread: Option<ThreadPointer>,
	event_queue: Queue,
}

impl SchedulerMut for TicklessRoundRobin {
	fn enqueue(&mut self, thread: ThreadPointer) {
		self.run_queue.push_back(thread);
		// todo: yield
	}

	fn current_thread(&mut self) -> Option<ThreadId> {
		self.current_thread.as_mut()
				.map(|t| *t.tcb().thread_id)
	}

	fn prepare_switch_thread(&mut self) -> (PointerView<'_>, PointerView<'_>) {
		if let Some(new_thread) = self.run_queue.pop_front() {
			let old_tcb = {
				let old_thread = self.current_thread.replace(new_thread)
				    .expect("Must be currently running on a thread");
				// todo: don't put back if blocked
				self.run_queue.push_back(old_thread);
				self.run_queue.back_mut()
						.expect("Just added a new thread")
						.tcb()
			};
			
			let new_tcb = self.current_thread.as_mut()
					.expect("Just added a new thread")
					.tcb();

			(old_tcb, new_tcb)
		} else {
			#[cfg(feature = "log.scheduler")] debug!("No other tasks");
			
			todo!()
		}
	}

	fn tick(&mut self, set_timer: fn(Instant) -> Result<(), Box<dyn Debug>>) {
		todo!()
	}

	fn post_switch_thread(&mut self) {
		todo!()
	}

	unsafe fn thread_startup(&mut self) {}
}

impl TicklessRoundRobin {
	pub fn new(running_thread: ThreadPointer) -> Self {
		let ret = Self {
			run_queue: VecDeque::new(),
			current_thread: Some(running_thread),
			event_queue: Queue::new(),
		};
		ret
	}
}
