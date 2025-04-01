#[allow(unused_imports)] use crate::prelude::*;
use alloc::collections::VecDeque;
use alloc::sync::{Arc, Weak};
use core::fmt::Debug;
use crate::threading::scheduler::{Scheduler, SchedulerSwitchState};
use crate::threading::{ThreadPointer, WakeReason};
use event::{Queue, Event};
use kernel_api::sync::{IrqGuard, Spinlock};
use crate::threading;
use crate::threading::{PointerView, ThreadState, ThreadId};

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

	fn switch_thread_pre(&mut self) -> SchedulerSwitchState<'_> {
		if let Some(new_thread) = self.run_queue.lock().pop_front() {
			let old_thread = self.current_thread.replace(new_thread);
			
			let new_thread = self.current_thread.as_mut()
			                     .expect("Just added a new thread")
			                     .tcb_mut();

			if let Some(old_thread) = old_thread {
				SchedulerSwitchState::Switch {
					old_thread,
					new_thread
				}
			} else {
				SchedulerSwitchState::SwitchFromIdle {
					new_thread
				}
			}
		} else {
			#[cfg(feature = "log.scheduler")] debug!("No other tasks");
			match self.current_thread.as_mut() {
				Some(current_tcb) => if current_tcb.tcb_mut().state.is_running() || current_tcb.tcb_mut().state.is_ready() {
					#[cfg(feature = "log.scheduler")] debug!("Can keep running existing thread");
					SchedulerSwitchState::NoSwitch
				} else {
					#[cfg(feature = "log.scheduler")] debug!("Idling");
					let old_thread = self.current_thread.take()
					                     .expect("Must be currently running on a thread");
					SchedulerSwitchState::Idle { old_thread }
				},
				None => SchedulerSwitchState::NoSwitch,
			}
		}
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
		let t = self.current_thread.as_mut().expect("`tickless_round_robin` globally parks all threads so unpark a local thread must be the current thread");
		assert_eq!(*t.tcb_ref().thread_id, thread_id);
		debug_assert!(t.tcb_mut().state.is_parked() || t.tcb_mut().state.is_just_unparked());
		*t.tcb_mut().state = ThreadState::JustUnparked(reason);
	}
}
