#[allow(unused_imports)] use crate::prelude::*;
use alloc::collections::VecDeque;
use alloc::sync::{Arc, Weak};
use core::fmt::Debug;
use crate::threading::scheduler::Scheduler;
use crate::threading::ThreadPointer;
use event::{Queue, Event};
use kernel_api::sync::{IrqGuard, Spinlock};
use crate::threading;
use crate::threading::{PointerView, ThreadState};

mod event;

#[derive(Debug)]
pub struct TicklessRoundRobin {
	run_queue: Arc<Spinlock<VecDeque<ThreadPointer>>>,
	current_thread: Option<ThreadPointer>,
	event_queue: Queue,
}

#[derive(Debug)]
pub struct Injector {
	queue: Weak<Spinlock<VecDeque<ThreadPointer>>>
}

impl super::Injector for Injector {
	fn enqueue(&self, thread: ThreadPointer) {
		let queue = match self.queue.upgrade() {
			Some(queue) => queue,
			None => {
				warn!("Attempted to inject into dead task queue");
				return;
			},
		};
		
		queue.lock()
				.push_back(thread);
	}
}

#[derive(Debug)]
pub struct Stealer {}
impl super::Stealer for Stealer {}

impl Scheduler for TicklessRoundRobin {
	fn new(running_thread: ThreadPointer) -> (Self, Box<dyn super::Injector>, Arc<dyn super::Stealer>) where Self: Sized {
		let this = Self {
			run_queue: Arc::new(Spinlock::new(VecDeque::new())),
			current_thread: Some(running_thread),
			event_queue: Queue::new(),
		};
		let queue = Arc::downgrade(&this.run_queue);
		
		(
			this,
			Box::new(Injector { queue }),
			Arc::new(Stealer {})
		)
	}

	fn current_thread(&mut self) -> Option<PointerView<'_>> {
		self.current_thread.as_mut()
		    .map(|t| t.tcb_mut())
	}

	fn switch_thread_pre(&mut self) -> (ThreadPointer, PointerView<'_>) {
		if let Some(new_thread) = self.run_queue.lock().pop_front() {
			let old_thread = self.current_thread.replace(new_thread)
			                     .expect("Must be currently running on a thread");

			let new_tcb = self.current_thread.as_mut()
			                  .expect("Just added a new thread")
			                  .tcb_mut();
			
			(old_thread, new_tcb)
		} else {
			#[cfg(feature = "log.scheduler")] debug!("No other tasks");

			todo!()
		}
	}

	fn switch_thread_post(mut self: IrqGuard<Self>, mut old_thread: ThreadPointer) {
		debug_assert!(!old_thread.tcb_mut().state.is_running());
		match *old_thread.tcb_mut().state {
			ThreadState::Parked(_) => threading::move_to_global_parking_lot(old_thread),
			ThreadState::Ready | ThreadState::JustUnparked(_) => self.enqueue(old_thread),
			ThreadState::Running => unreachable!(),
		}
	}

	fn enqueue(&mut self, thread: ThreadPointer) {
		self.run_queue.lock().push_back(thread);
	}
}
