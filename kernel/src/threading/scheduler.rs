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

use alloc::sync::Arc;
use core::fmt::Debug;
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "preemptive")] use core::time::Duration;
use kernel_api::sync::{IrqCell, RwSpinlock};
use kernel_api::threading::ThreadId;
use crate::threading::{CoreId, ThreadControlBlock};

#[doc(hidden)]
mod tickless_round_robin;

/// [`Injector`]s to add new threads to each core
static SCHEDULER_INJECTORS: RwSpinlock<Vec<Box<dyn Injector>>> = RwSpinlock::new(vec![]);

pub trait Injector: Send + Sync {
	/// Adds the `thread` to the list of ready-to-run threads in the scheduler
	fn enqueue(&self, thread: ThreadControlBlock) -> Result<(), ThreadControlBlock>;
}

pub trait Stealer: Send + Sync {}

/// A scheduler implementation
pub trait Scheduler: Debug {
	/// Creates a new instance of the scheduler
	fn new() -> (Self, Box<dyn Injector>, Arc<dyn Stealer>) where Self: Sized;

	/// Prepares to switch threads
	fn get_next_thread_(&mut self) -> Option<ThreadControlBlock>;
	
	fn get_next_thread(&mut self) -> Option<ThreadControlBlock> {
		self.get_next_thread_()
	}

	/// Called after switching threads, including during startup of a new thread
	///
	/// `previous_thread` is the thread that was running before the context switch took place,
	/// and should be placed back on the run-list if it is not blocked.
	/// 
	/// A [`PointerView`] to the thread is returned
	fn put_thread(&mut self, previous_thread: ThreadControlBlock);

	fn on_thread_exit(&mut self, tid: ThreadId);
	
	/// Enqueues the passed `thread`
	///
	/// This may be called if enqueuing a thread from the same core the scheduler is on, as optimizations
	/// to reduce locking may be possible
	fn enqueue(&mut self, thread: ThreadControlBlock);

	fn unpark(&mut self, thread_id: ThreadId);
	//fn kill(&mut self, thread_id: ThreadId) -> Result<(), ()>;
}

#[define_opaque(super::SchedulerTy)]
pub(super) fn create_scheduler_for_current_core() -> CoreId {
	let (scheduler, injector, _stealer) = <tickless_round_robin::TicklessRoundRobin as Scheduler>::new();
	percpu_v2!(scheduler).set(IrqCell::new(scheduler))
			.expect("Scheduler already initialised");
	let mut guard = SCHEDULER_INJECTORS.write();
	guard.push(injector);
	CoreId { id: (guard.len() - 1).try_into().expect("too many cores") }
}

pub(super) fn local_scheduler() -> &'static IrqCell<impl Scheduler> {
	percpu_v2!(scheduler).get()
			.expect("Scheduler not yet initialised")
}

/// Enqueues a thread onto a core such that system load stays balanced
pub fn enqueue(mut thread: ThreadControlBlock) {
	static CORE_NUM: AtomicUsize = AtomicUsize::new(0);
	
	let injectors = SCHEDULER_INJECTORS.read();
	assert!(!injectors.is_empty(), "Scheduler not yet initialised");

	assert!(thread.state.runnable());
	
	loop {
		let injector_idx = CORE_NUM.fetch_add(1, Ordering::Relaxed) % injectors.len();
		let injector = &injectors[injector_idx];
		debug!("Inject into core {injector_idx}");

		// fixme: this needs to send an IPI to the corresponding core in case it's idling and needs waking up
		match injector.enqueue(thread) {
			Ok(_) => return,
			Err(err_thread) => thread = err_thread,
		}
	}
}
