//! This module provides traits for writing scheduler implementations, and the global helper methods for
//! interacting with schedulers
//!
//! A scheduler implementation is made from three objects:
//! - [`Scheduler`], implementing the actual logic of deciding which thread to run next
//! - [`Injector`], a global handle allowing adding new threads from a different core
//! - [`Stealer`], a global handle allowing [`Ready`](super::ThreadState::Ready) threads to be removed from the scheduler for
//! load balancing
//!
//! See the [book](https://popcorn-2.github.io/book) for an example of writing a scheduler from scratch.

#[allow(unused_imports)] use crate::prelude::*;
use alloc::sync::Arc;
use core::cell::OnceCell;
use core::fmt::Debug;
use core::mem;
use core::mem::transmute;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::hal;
#[cfg(feature = "preemptive")] use core::time::Duration;
use crossbeam_queue::SegQueue;
use kernel_api::sync::{IrqCell, IrqGuard, RwSpinlock, Spinlock};
use kernel_api::time::Instant;
use crate::hal::paging2::TTable;
use crate::memory::paging::ktable;
use crate::{assert_unsafe_precondition, hashmap_new, non_zero};
use crate::threading::{CoreId, PointerState, Thread, ThreadId, ThreadPointer, WakeReason};
use crate::threading::{PointerView, SharedView, ThreadControlBlock, ThreadState};
pub use control::ControlEvent;

#[doc(hidden)]
mod tickless_round_robin;
mod control;

/// [`Injector`]s to add new threads to each core
static SCHEDULER_INJECTORS: RwSpinlock<Vec<Box<dyn Injector>>> = RwSpinlock::new(vec![]);

static SCHEDULER_EVENT_QUEUES: RwSpinlock<Vec<Arc<SegQueue<ControlEvent>>>> = RwSpinlock::new(vec![]);

pub trait Injector: Send + Sync {
	/// Adds the `thread` to the list of ready-to-run threads in the scheduler
	fn enqueue(&self, thread: ThreadPointer);
}

pub trait Stealer: Send + Sync {}

/// A scheduler implementation
pub trait Scheduler: Debug {
	/// Creates a new instance of the scheduler
	fn new() -> (Self, Box<dyn Injector>, Arc<dyn Stealer>) where Self: Sized;

	/// Prepares to switch threads
	fn get_next_thread_(&mut self) -> Option<ThreadPointer>;
	
	fn get_next_thread(&mut self, control_queue: &SegQueue<ControlEvent>) -> Option<ThreadPointer> {
		while let Some(event) = control_queue.pop() {
			handle_control_event(self, event);
		}
		self.get_next_thread_()
	}

	/// Called after switching threads, including during startup of a new thread
	///
	/// `previous_thread` is the thread that was running before the context switch took place,
	/// and should be placed back on the run-list if it is not blocked.
	/// 
	/// A [`PointerView`] to the thread is returned
	fn switch_thread_post(&mut self, previous_thread: ThreadPointer);

	/// Enqueues the passed `thread`
	///
	/// This may be called if enqueuing a thread from the same core the scheduler is on, as optimizations
	/// to reduce locking may be possible
	fn enqueue(&mut self, thread: ThreadPointer);

	fn unpark(&mut self, thread_id: ThreadId, reason: WakeReason) -> Result<(), ()>;
	fn kill(&mut self, thread_id: ThreadId) -> Result<(), ()>;
}

pub fn handle_control_event(this: &mut (impl Scheduler + ?Sized), event: ControlEvent) {
	debug!("handle scheduler event {event:?}");
	match event {
		ControlEvent::Unpark(thread_id, reason) => {
			match this.unpark(thread_id, reason) {
				Ok(_) => {},
				Err(_) => super::parking::do_wake(thread_id, reason),
			}
		},
		ControlEvent::Kill(thread_id) => {
			match this.kill(thread_id) {
				Ok(_) => {},
				Err(_) => super::do_kill(thread_id),
			}
		}
	}
}

pub fn send_control_event(core: CoreId, event: ControlEvent) {
	debug!("send control event {event:?} to core {core:?}");
	SCHEDULER_EVENT_QUEUES.read()[core.id as usize].push(event);
}

#[define_opaque(super::SchedulerTy)]
pub(super) fn create_scheduler_for_current_core() -> CoreId {
	let (scheduler, injector, _stealer) = <tickless_round_robin::TicklessRoundRobin as Scheduler>::new();
	let control_queue = Arc::new(SegQueue::new());
	percpu_v2!(scheduler).set((IrqCell::new(scheduler), Arc::clone(&control_queue)))
			.expect("Scheduler already initialised");
	let mut guard = SCHEDULER_INJECTORS.write();
	guard.push(injector);
	let mut guard = SCHEDULER_EVENT_QUEUES.write();
	guard.push(control_queue);
	CoreId { id: (guard.len() - 1).try_into().expect("too many cores") }
}

pub(super) fn local_scheduler() -> (&'static IrqCell<impl Scheduler>, &'static SegQueue<ControlEvent>) {
	let (scheduler, queue) = percpu_v2!(scheduler).get()
			.expect("Scheduler not yet initialised");
	(scheduler, queue)
}

/// Enqueues a thread onto a core such that system load stays balanced
pub fn enqueue(thread: &mut PointerState) {
	static CORE_NUM: AtomicUsize = AtomicUsize::new(0);
	
	let injectors = SCHEDULER_INJECTORS.read();
	assert!(!injectors.is_empty(), "Scheduler not yet initialised");
	let injector_idx = CORE_NUM.fetch_add(1, Ordering::Relaxed) % injectors.len();
	let injector = &injectors[injector_idx];
	debug!("Inject into core {injector_idx}");

	let PointerState::GloballyParked(mut ptr) = mem::replace(thread, PointerState::InScheduler(CoreId { id: injector_idx as isize })) else { unreachable!() };

	assert!(ptr.tcb_mut().state.is_ready());

	// fixme: this needs to send an IPI to the corresponding core in case it's idling and needs waking up
	injector.enqueue(ptr);
}
