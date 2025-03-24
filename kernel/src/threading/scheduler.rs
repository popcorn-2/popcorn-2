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
use crate::hal::timing::{Timer, Eoi};
use crate::interrupts::irq_handler;
use crate::memory::paging::ktable;
use crate::{hashmap_new, non_zero, assert_unsafe_precondition};
use crate::threading::{CoreId, Thread, ThreadId, ThreadPointer, WakeReason};
use crate::threading::{PointerView, SharedView, ThreadControlBlock, ThreadState};

#[doc(hidden)]
mod tickless_round_robin;

/// The [`Scheduler`] for the current core
#[thread_local]
static SCHEDULER: OnceCell<IrqCell<tickless_round_robin::TicklessRoundRobin>> = OnceCell::new();

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
	///
	/// Returns an owned [`ThreadPointer`] to the currently running thread, and a [`PointerView`] to the thread to switch to.
	/// Ownership of the [`ThreadPointer`] will be given back to the scheduler in [`Scheduler::switch_thread_post()`].
	fn switch_thread_pre(&mut self) -> (ThreadPointer, PointerView<'_>);

	/// Called after switching threads, including during startup of a new thread
	///
	/// `previous_thread` is the thread that was running before the context switch took place,
	/// and should be placed back on the run-list if it is not blocked.
	fn switch_thread_post(self: IrqGuard<Self>, previous_thread: ThreadPointer); // do we want to dispatch on `IrqGuard`? - it's supposed to enforce proper usage of switch_thread

	/// Enqueues the passed `thread`
	///
	/// This may be called if enqueuing a thread from the same core the scheduler is on, as optimizations
	/// to reduce locking may be possible
	fn enqueue(&mut self, thread: ThreadPointer) {
		let _ = thread;
		todo!("");
	}
}

pub(super) fn create_scheduler_for_current_core(running_thread: ThreadPointer) -> CoreId {
	debug_assert!(SCHEDULER.get().is_none(), "Scheduler already initialised");
	let (scheduler, injector, _stealer) = <tickless_round_robin::TicklessRoundRobin as Scheduler>::new(running_thread);
	SCHEDULER.set(IrqCell::new(scheduler))
			.expect("Scheduler already initialised");
	let mut guard = SCHEDULER_INJECTORS.lock();
	guard.push(injector);
	CoreId { id: guard.len() - 1 }
}

pub(super) fn local_scheduler() -> &'static IrqCell<impl Scheduler + Debug> {
	let s = SCHEDULER.get()
			.expect("Scheduler not yet initialised");
	unsafe { transmute::<&IrqCell<tickless_round_robin::TicklessRoundRobin>, &'static IrqCell<tickless_round_robin::TicklessRoundRobin>>(s) }
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
	injector.enqueue(thread);
}
