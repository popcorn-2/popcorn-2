#[allow(unused_imports)] use crate::prelude::*;
use alloc::collections::VecDeque;
use alloc::sync::{Arc, Weak};
use core::fmt::Debug;
use crate::threading::scheduler::Scheduler;
use crate::threading::{ThreadPointer, WakeReason};
use event::{Queue, Event};
use kernel_api::sync::{IrqGuard, Spinlock};
use crate::threading;
use crate::threading::{PointerView, ThreadState, ThreadId};

mod event;

#[derive(Debug)]
pub struct TicklessRoundRobin {
	run_queue: Arc<Spinlock<VecDeque<ThreadPointer>>>,
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
	fn new() -> (Self, Box<dyn super::Injector>, Arc<dyn super::Stealer>) where Self: Sized {
		let this = Self {
			run_queue: Arc::new(Spinlock::new(VecDeque::new())),
			event_queue: Queue::new(),
		};
		let queue = Arc::downgrade(&this.run_queue);
		
		(
			this,
			Box::new(Injector { queue }),
			Arc::new(Stealer {})
		)
	}

	fn get_next_thread(&mut self) -> Option<ThreadPointer> {
		self.run_queue.lock().pop_front()
	}

	fn switch_thread_post(&mut self, mut old_thread: ThreadPointer) {
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

	fn unpark(&mut self, thread_id: ThreadId, reason: WakeReason) {
		debug!("scheduler local unpark of {thread_id:?} for {reason:?}");
		let mut guard = percpu_v2!(current_thread).write();
		let t = guard.as_mut().expect("`tickless_round_robin` globally parks all threads so unpark a local thread must be the current thread");
		assert_eq!(*t.tcb_ref().thread_id, thread_id);
		debug_assert!(t.tcb_mut().state.is_parked() || t.tcb_mut().state.is_just_unparked());
		*t.tcb_mut().state = ThreadState::JustUnparked(reason);
	}
}
