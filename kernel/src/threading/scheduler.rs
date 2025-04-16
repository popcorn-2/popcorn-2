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
use core::mem::transmute;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::hal;
#[cfg(feature = "preemptive")] use core::time::Duration;
use kernel_api::sync::{IrqCell, IrqGuard, Spinlock};
use kernel_api::time::Instant;
use crate::hal::paging2::TTable;
use crate::memory::paging::ktable;
use crate::{hashmap_new, non_zero, assert_unsafe_precondition};
use crate::threading::{CoreId, Thread, ThreadId, ThreadPointer, WakeReason};
use crate::threading::{PointerView, SharedView, ThreadControlBlock, ThreadState};

#[doc(hidden)]
mod tickless_round_robin;

/// [`Injector`]s to add new threads to each core
static SCHEDULER_INJECTORS: Spinlock<Vec<Box<dyn Injector>>> = Spinlock::new(vec![]);

pub trait Injector: Send + Sync {
	/// Adds the `thread` to the list of ready-to-run threads in the scheduler
	fn enqueue(&self, thread: ThreadPointer);
}

pub trait Stealer: Send + Sync {}

/// A scheduler implementation
pub trait Scheduler: Debug {
	/// Creates a new instance of the scheduler
	fn new(running_thread: ThreadPointer) -> (Self, Box<dyn Injector>, Arc<dyn Stealer>) where Self: Sized;

	/// Returns the [`ThreadId`] for the currently running thread
	///
	/// Returns `None` if idle
	fn current_thread(&mut self) -> Option<PointerView<'_>>;

	/// Prepares to switch threads
	fn switch_thread_pre(&mut self) -> SchedulerSwitchState<'_>;

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

	fn unpark(&mut self, thread_id: ThreadId, reason: WakeReason);
}

pub enum SchedulerSwitchState<'a> {
	Switch {
		/// The previously running thread, which will be passed back to the scheduler in [`Scheduler::switch_thread_post()`]
		/// after the context switch occurs
		old_thread: ThreadPointer,
		/// The new thread to switch to
		new_thread: PointerView<'a>,
	},
	/// No new threads to run, and the current thread has blocked
	Idle {
		/// The previously running thread
		old_thread: ThreadPointer,
	},
	/// No new threads to run but the previously running thread is still in a running state
	NoSwitch,
	SwitchFromIdle { new_thread: PointerView<'a> },
}

#[define_opaque(super::SchedulerTy)]
pub(super) fn create_scheduler_for_current_core(running_thread: ThreadPointer) -> CoreId {
	let (scheduler, injector, _stealer) = <tickless_round_robin::TicklessRoundRobin as Scheduler>::new(running_thread);
	percpu_v2!(scheduler).set(IrqCell::new(scheduler))
			.expect("Scheduler already initialised");
	let mut guard = SCHEDULER_INJECTORS.lock();
	guard.push(injector);
	CoreId { id: guard.len() - 1 }
}

pub(super) fn local_scheduler() -> &'static IrqCell<impl Scheduler + Debug> {
	percpu_v2!(scheduler).get()
			.expect("Scheduler not yet initialised")
}

/// Enqueues a thread onto a core such that system load stays balanced
pub fn enqueue(mut thread: ThreadPointer) {
	static CORE_NUM: AtomicUsize = AtomicUsize::new(0);
	
	assert!(thread.tcb_mut().state.is_ready());
	
	let injectors = SCHEDULER_INJECTORS.lock();
	assert!(!injectors.is_empty(), "Scheduler not yet initialised");
	let injector_idx = CORE_NUM.fetch_add(1, Ordering::Relaxed) % injectors.len();
	let injector = &injectors[injector_idx];
	debug!("Inject into core {injector_idx}");

	// fixme: this needs to send an IPI to the corresponding core in case it's idling and needs waking up
	injector.enqueue(thread);
}
