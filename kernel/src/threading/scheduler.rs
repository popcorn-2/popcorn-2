#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, VecDeque};
use core::borrow::Borrow;
use core::cell::{Cell, OnceCell, UnsafeCell};
use core::cmp::min;
use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, transmute};
use core::num::NonZero;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use crate::hal;
#[cfg(feature = "preemptive")] use core::time::Duration;
use hashbrown::HashMap;
use kernel_api::memory::physical::highmem;
use kernel_api::sync::{IrqCell, IrqGuard, Mutex};
use kernel_api::time::Instant;
use crate::hal::paging2::TTable;
use crate::hal::timing::{Timer, Eoi};
use crate::interrupts::irq_handler;
use crate::memory::paging::ktable;
use crate::{hashmap_new, non_zero, assert_unsafe_precondition};
use crate::threading::{Thread, ThreadId, ThreadPointer};
use crate::threading::tcb::{PointerView, ThreadControlBlock};

mod tickless_round_robin;

#[thread_local]
static SCHEDULER: OnceCell<IrqCell<tickless_round_robin::TicklessRoundRobin>> = OnceCell::new();

pub static TASK_LIST: Mutex<HashMap<ThreadId, Thread>> = Mutex::new(hashmap_new!());

macro_rules! __scheduler_traits_sig {
    (@ref $(#[$attr:meta])* fn $name:ident($($arg_i:ident: $arg_ty:ty),* $(,)?) $(-> $ret:ty)?) => {
	    $(#[$attr])* fn $name(&self $(, $arg_i: $arg_ty)*) $(-> $ret)?;
    };
    (@ref $(#[$attr:meta])* unsafe fn $name:ident($($arg_i:ident: $arg_ty:ty),* $(,)?) $(-> $ret:ty)?) => {
	    $(#[$attr])* unsafe fn $name(&self $(, $arg_i: $arg_ty)*) $(-> $ret)?;
    };
    (@mut $(#[$attr:meta])* fn $name:ident($($arg_i:ident: $arg_ty:ty),* $(,)?) $(-> $ret:ty)?) => {
	    $(#[$attr])* fn $name(&mut self $(, $arg_i: $arg_ty)*) $(-> $ret)?;
    };
    (@mut $(#[$attr:meta])* unsafe fn $name:ident($($arg_i:ident: $arg_ty:ty),* $(,)?) $(-> $ret:ty)?) => {
	    $(#[$attr])* unsafe fn $name(&mut self $(, $arg_i: $arg_ty)*) $(-> $ret)?;
    };
    (auto fn $name:ident($($arg_i:ident: $arg_ty:ty),*) $(-> $ret:ty)?) => {
	    fn $name($($arg_i: $arg_ty),*) $(-> $ret)? {
			let mut guard = self.lock();
			guard.$name($($arg_i),*)
		}
    };
    (auto unsafe fn $name:ident($($arg_i:ident: $arg_ty:ty),*) $(-> $ret:ty)?) => {
	    unsafe fn $name($($arg_i: $arg_ty),*) $(-> $ret)? {
			let mut guard = self.lock();
			unsafe { guard.$name($($arg_i),*) }
		}
    };
	(impl fn $name:ident($($arg_i:ident: $arg_ty:ty),*) $(-> $ret:ty)? $blk:block) => {
	    fn $name($($arg_i: $arg_ty),*) $(-> $ret)? $blk
    };
	(impl unsafe fn $name:ident($($arg_i:ident: $arg_ty:ty),*) $(-> $ret:ty)? $blk:block) => {
	    unsafe fn $name($($arg_i: $arg_ty),*) $(-> $ret)? $blk
    };
}

macro_rules! scheduler_traits {
    ($($(#[$attr:meta])* $f1:ident $f2:ident $f3:ident $($f4:ident)? (&self $(, $arg_i:ident: $arg_ty:ty)* $(,)?) $(-> $ret:ty)? $($blk:block)?;)*) => {
	    pub trait Scheduler {
		    $(__scheduler_traits_sig!(@ref $(#[$attr])* $f2 $f3 $($f4)? ($($arg_i: $arg_ty),*) $(-> $ret)?);)*
	    }

	    trait SchedulerMut {
		    $(__scheduler_traits_sig!(@mut $(#[$attr])* $f2 $f3 $($f4)? ($($arg_i: $arg_ty),*) $(-> $ret)?);)*
	    }

	    /*impl<T: SchedulerMut> Scheduler for IrqCell<T> {
			$(__scheduler_traits_sig!{$f1 $f2 $f3 $($f4)? (self: &Self $(, $arg_i: $arg_ty)*) $(-> $ret)? $($blk)?})*
		}*/
    };
}

scheduler_traits! {
	auto fn enqueue(&self, thread: ThreadPointer);
	auto fn current_thread(&self) -> Option<ThreadId>;

	// TODO: is this sound, and how can it be more safe
	impl fn prepare_switch_thread(&self) -> (PointerView<'_>, PointerView<'_>) {
		let mut guard = ManuallyDrop::new(self.lock());
		let ptrs = guard.prepare_switch_thread();
		let ptrs = unsafe { (transmute::<PointerView<'_>, PointerView<'static>>(ptrs.0), transmute::<PointerView<'_>, PointerView<'static>>(ptrs.1)) };
		ptrs
	};

	impl fn post_switch_thread(&self) {
		todo!()
	};

	auto fn tick(&self, set_timer: fn(Instant) -> Result<(), Box<dyn Debug>>);

	impl unsafe fn thread_startup(&self) {
		todo!();
	};
}

impl<T: SchedulerMut> Scheduler for IrqCell<T> {
	fn enqueue(&self, thread: ThreadPointer) { self.lock().enqueue(thread) }

	fn current_thread(&self) -> Option<ThreadId> { self.lock().current_thread() }

	fn prepare_switch_thread<'a>(&'a self) -> (PointerView<'a>, PointerView<'a>) {
		let mut this = ManuallyDrop::new(self.lock());
		let (a, b) = this.prepare_switch_thread();
		let a = unsafe { transmute::<PointerView, PointerView<'a>>(a) };
		let b = unsafe { transmute::<PointerView, PointerView<'a>>(b) };
		(a, b)
	}

	fn post_switch_thread(&self) {
		unsafe { self.unlock(); }
	}

	fn tick(&self, set_timer: fn(Instant) -> Result<(), Box<dyn Debug>>) {
		todo!()
	}

	unsafe fn thread_startup(&self) {
		let mut guard = unsafe { self.make_guard_unchecked() };
		guard.thread_startup();
	}
}

/// # Safety
/// This must only be called once per core
pub(super) unsafe fn create_scheduler_for_current_core(running_thread: ThreadPointer) {
	let scheduler = tickless_round_robin::TicklessRoundRobin::new(running_thread);
	let res = SCHEDULER.set(IrqCell::new(scheduler));
	assert_unsafe_precondition!("Scheduler already initialised", (ok: bool = res.is_ok()) => ok);
	res.unwrap_unchecked();
}

pub(super) fn scheduler() -> &'static (impl Scheduler + Debug) {
	let s = SCHEDULER.get()
			.expect("Scheduler not yet initialised");
	unsafe { core::mem::transmute::<&IrqCell<tickless_round_robin::TicklessRoundRobin>, &'static IrqCell<tickless_round_robin::TicklessRoundRobin>>(s) }
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
	let scheduler = scheduler(); // todo: pick a particular core somehow
	scheduler.enqueue(thread);
}
