#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use core::borrow::Borrow;
use core::cell::{Cell, OnceCell, UnsafeCell};
use core::cmp::min;
use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, transmute};
use core::num::NonZero;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::hal;
#[cfg(feature = "preemptive")] use core::time::Duration;
use hashbrown::HashMap;
use kernel_api::memory::physical::highmem;
use kernel_api::sync::{IrqCell, IrqGuard, Spinlock};
use kernel_api::time::Instant;
use crate::hal::paging2::TTable;
use crate::hal::timing::{Timer, Eoi};
use crate::interrupts::irq_handler;
use crate::memory::paging::ktable;
use crate::{hashmap_new, non_zero, assert_unsafe_precondition};
use crate::threading::{CoreId, Thread, ThreadId, ThreadPointer};
use crate::threading::tcb::{PointerView, ThreadControlBlock};

mod tickless_round_robin;

#[thread_local]
static SCHEDULER: OnceCell<IrqCell<tickless_round_robin::TicklessRoundRobin>> = OnceCell::new();

/// Global list of all running threads
pub static TASK_LIST: Spinlock<HashMap<ThreadId, Thread>> = Spinlock::new(hashmap_new!());

/// [`Injector`]s to add new threads to each core
static SCHEDULER_INJECTORS: Spinlock<Vec<Box<dyn Injector>>> = Spinlock::new(vec![]);

pub trait Injector: Send + Sync {
	fn enqueue(&self, thread: ThreadPointer);
}

pub trait Stealer: Send + Sync {}

pub trait Scheduler: Debug {
	fn new(running_thread: ThreadPointer) -> (Self, Box<dyn Injector>, Arc<dyn Stealer>) where Self: Sized;
	fn current_thread(&self) -> Option<ThreadId>;
	fn switch_thread_pre(&mut self) -> (ThreadPointer, PointerView<'_>);
	fn switch_thread_post(self: IrqGuard<Self>, previous_thread: ThreadPointer); // do we want to dispatch on `IrqGuard`? - it's supposed to enforce proper usage of switch_thread
	fn enqueue(&mut self, thread: ThreadPointer);
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

pub(super) fn scheduler() -> &'static IrqCell<impl Scheduler + Debug> {
	let s = SCHEDULER.get()
			.expect("Scheduler not yet initialised");
	unsafe { transmute::<&IrqCell<tickless_round_robin::TicklessRoundRobin>, &'static IrqCell<tickless_round_robin::TicklessRoundRobin>>(s) }
}

pub(super) fn enqueue(tcb: ThreadControlBlock) {
	let id = tcb.thread_id;
	let (thread, ptr) = ThreadPointer::new(Thread::new(tcb));

	TASK_LIST.lock()
			.try_insert(id, thread)
			.expect("ThreadId reuse");

	enqueue_balanced(ptr);
}

fn enqueue_balanced(thread: ThreadPointer) {
	static CORE_NUM: AtomicUsize = AtomicUsize::new(0);

	let injectors = SCHEDULER_INJECTORS.lock();
	assert!(!injectors.is_empty(), "Scheduler not yet initialised");
	let injector_idx = CORE_NUM.fetch_add(1, Ordering::Relaxed) % injectors.len();
	let injector = &injectors[injector_idx];
	debug!("Inject into core {injector_idx}");
	injector.enqueue(thread);
}
