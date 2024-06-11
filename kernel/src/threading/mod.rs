//! This module provides the thread API
//!
//! # [`ThreadControlBlock`] vs [`Thread`] vs [`ThreadPointer`] vs [`ThreadId`]
//!
//! [`ThreadControlBlock`] holds the underlying state of a thread, including the saved register
//! state. This is a mix of core kernel types and types exposed via the [HAL](crate::hal).
//! Each [`ThreadControlBlock`] is stored on the heap, and is effectively pointed to by [`Thread`],
//! [`ThreadPointer`] and [`ThreadId`].
//!
//! [`ThreadId`] is a cheap to copy handle to a particular thread, which does not give access
//! to the underlying [`ThreadControlBlock`]. Instead, a limited API is provided through methods on it,
//! similar in scope to thread control methods available in userspace. This is designed for use outside
//! the scheduler itself, for example being used in [wait queues](crate::threading::wait_queue). While
//! unlikely, if a [`ThreadId`] is held for a long time, the underlying thread may have exit, and the
//! [`ThreadId`] reused. In this case, any actions will affect the new thread. The limited API surface
//! provided is designed to guard against this causing any problems.
//!
//! [`Thread`] and [`ThreadPointer`] are both pointers to an underlying [`ThreadControlBlock`]. [`Thread`]
//! 'owns' the allocation, and a single [`ThreadPointer`] can be borrowed from it at any one time. These
//! both provide access to most of the fields of [`ThreadControlBlock`] (see the [safety](#threadpointer-safety) section below
//! for more information). [`Thread`]s are stored in the global task list, while [`ThreadPointer`]s are
//! passed to scheduler implementations for use within run-queues.
//!
//! Together, [`Thread`] and [`ThreadPointer`] are somewhat analogous to [`Arc<ThreadControlBlock>`](alloc::sync::Arc)
//! whereas [`ThreadId`] is like a [`Weak<ThreadControlBlock>`](alloc::sync::Weak).
//!
//! # [`ThreadPointer`] safety
//!
//! Each [`ThreadControlBlock`] is pointed to by an [`Thread`] pointer from a global thread list.
//! This pointer can be 'borrowed' to a [`ThreadPointer`] to be placed into run queues. Only once
//! instance of an [`Thread`] can ever exist for the same [`ThreadControlBlock`], and either zero
//! or one [`ThreadPointer`]s to the same [`ThreadControlBlock`].
//!
//! Since the pointers are shared pointers, both pointers only provide immutable access to most of
//! the inner [`ThreadControlBlock`] fields. [`save_state`](ThreadControlBlock::save_state) is an exception to
//! this. This can only be accessed through [`ThreadPointer`], making it safe to provide mutable access. This
//! removes any lock contention during context switches.

#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use core::arch::{asm, naked_asm};
use core::cmp::Ordering;
use core::fmt::{Debug, Formatter};
use core::mem;
use core::num::NonZero;
use core::ptr::{addr_of, DynMetadata, NonNull};
use core::sync::atomic::AtomicUsize;
use core::time::Duration;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::{highmem, OwnedFrames};
use kernel_api::memory::r#virtual::{Global, OwnedPages};
use kernel_api::time::Instant;
use crate::threading::tcb::{ThreadControlBlock, ThreadState};
use crate::hal::{Hal, HalTy, SaveState};
use crate::{non_zero, assert_unsafe_precondition};
use scheduler::Scheduler;
use crate::hal::paging2::{TTable, TTableTy};
use crate::memory::paging::ktable;

mod scheduler;
pub mod tcb;
mod cleanup;

const INIT_THREAD: ThreadId = ThreadId { id: non_zero!(1) };

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ThreadId {
	id: NonZero<usize>,
}

impl ThreadId {
	fn new() -> Self {
		static THREAD_IDS: AtomicUsize = AtomicUsize::new(2);

		let id = THREAD_IDS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
		let id = NonZero::<usize>::new(id)
				.expect("`ThreadId` value overflowed");

		ThreadId {
			id
		}
	}
}

pub struct Thread {
	ptr: NonNull<ThreadControlBlock>,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

pub struct ThreadPointer {
	ptr: NonNull<ThreadControlBlock>,
}

unsafe impl Send for ThreadPointer {}
unsafe impl Sync for ThreadPointer {}

impl Thread {
	/// Constructs an Owned pointer to a [`ThreadControlBlock`]. See the [module level docs](self) for more information.
	pub fn new(tcb: ThreadControlBlock) -> Thread {
		let b = Box::new(tcb);
		let ptr = NonNull::from(Box::leak(b));
		Thread { ptr }
	}
}

impl ThreadPointer {
	/// # Safety
	///
	/// The caller must ensure that no other [`ThreadPointer`]s to the same [`ThreadControlBlock`] exist
	unsafe fn new_unchecked(owned: &Thread) -> ThreadPointer {
		ThreadPointer { ptr: owned.ptr }
	}

	fn new(owned: Thread) -> (Thread, ThreadPointer) {
		let ptr = unsafe { Self::new_unchecked(&owned) };
		(owned, ptr)
	}

	fn tcb(&mut self) -> tcb::PointerView<'_> {
		let tcb = unsafe { self.ptr.as_ref() };
		unsafe { tcb::PointerView::from_tcb(tcb) }
	}

	// Must take `&mut self` to prevent aliasing, as in the following example
	// ```no_run
	// let a = ptr.save_state();
	// let b = ptr.save_state(); // <- produces a multiple mutable borrow only with `&mut self`
	// f(a, b);
	// ```
	pub fn save_state(&mut self) -> &mut <HalTy as Hal>::SaveState {
		todo!()
		/*let ptr = self.tcb().save_state.get();
		unsafe { &mut *ptr }*/
	}
}

impl Debug for Thread {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let thread_id = unsafe { *addr_of!((*self.ptr.as_ptr()).thread_id) };
		f.debug_struct("Thread")
		 .field("ThreadId", &thread_id)
		 .finish_non_exhaustive()
	}
}

impl Debug for ThreadPointer {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let thread_id = unsafe { *addr_of!((*self.ptr.as_ptr()).thread_id) };
		f.debug_struct("ThreadPointer")
		 .field("ThreadId", &thread_id)
		 .finish_non_exhaustive()
	}
}

/// # Safety
/// This must only be called once during boot
pub unsafe fn init(handoff_data: crate::HandoffWrapper) -> ThreadId {
	assert_unsafe_precondition!("`threading::init` must only be called once", () => {
		static INIT: ::core::sync::atomic::AtomicBool = ::core::sync::atomic::AtomicBool::new(false);
		let ret = INIT.load(::core::sync::atomic::Ordering::Relaxed) == false;
		INIT.store(true, ::core::sync::atomic::Ordering::Relaxed);
		ret
	});

	let stack = handoff_data.memory.stack;
	let ttable = handoff_data.to_empty_ttable();

	// fixme: is highmem always correct?
	let stack_phys_len = stack.top_virt - stack.bottom_virt - 1;
	let stack_frames = OwnedFrames::from_raw_parts(
		stack.top_phys - stack_phys_len,
		NonZero::<usize>::new(stack_phys_len).expect("Cannot have a zero sized stack"),
		highmem()
	);
	let stack_pages = OwnedPages::from_raw_parts(
		stack.bottom_virt,
		NonZero::<usize>::new(stack_phys_len + 1).expect("Cannot have a zero sized stack"),
		Global
	);

	let tcb = ThreadControlBlock::new_inner(
		ttable,
		Default::default(),
		Cow::Borrowed("init"),
		Stack::from_contiguous_raw_parts(stack_frames, stack_pages),
		ThreadState::Running,
		INIT_THREAD,
	);
	let thread = Thread::new(tcb);
	let ptr = ThreadPointer { ptr: thread.ptr };
	assert!(
		scheduler::TASK_LIST.lock()
				.try_insert(INIT_THREAD, thread)
				.is_ok(),
		"ThreadId(1) should not exist already"
	);
	
	scheduler::create_scheduler_for_current_core(ptr);

	INIT_THREAD
}

pub fn thread_yield() {
	let s = scheduler::scheduler();
	let (from, to) = s.prepare_switch_thread();
	debug!("switch from `{:?}` to `{:?}`", from.thread_id, to.thread_id);
	unsafe { <HalTy as Hal>::switch_thread(&from, &to); }
	debug!("switch done");
	s.post_switch_thread();
	//defer_schedule();
}

pub fn spawn_with(f: impl FnOnce() + Send + 'static, name: Cow<'static, str>) -> ThreadId {
	extern "C" fn main((ptr, meta): (usize, usize)) -> ! {
		let ptr = ptr as *mut i8;
		let meta = unsafe { mem::transmute::<usize, DynMetadata<dyn FnOnce()>>(meta) };
		let ptr = core::ptr::from_raw_parts_mut::<dyn FnOnce()>(ptr, meta);
		unsafe { Box::from_raw(ptr)() };
		exit(0);
	}

	let (ptr, meta) = {
		let b = Box::new(f) as Box<dyn FnOnce()>;
		Box::into_raw(b).to_raw_parts()
	};

	let ttable = TTableTy::new(&*ktable(), highmem()).unwrap();
	let (tcb, id) = ThreadControlBlock::new(
		name,
		ttable,
		thread_startup,
		main,
		(ptr as usize, unsafe { mem::transmute::<DynMetadata<dyn FnOnce()>, usize>(meta) })
	);

	scheduler::enqueue(tcb);

	id
}

pub fn block(reason: ThreadState) {
	todo!()
}

pub fn current_thread() -> Option<ThreadId> {
	scheduler::scheduler().current_thread()
}

pub fn unblock(tid: ThreadId) {
	todo!()
}

fn push_to_global_sleep_queue(_wake_time: Instant) {
	todo!()
}

fn pinned_sleep(time_of_wake: Instant) {
	todo!();
	/* let mut guard = scheduler::SCHEDULER.lock();
	let sleep_event = SchedulerEvent {
		tid: guard.current_thread_id().unwrap(),
		time: time_of_wake,
		action: EventTy::Unblock
	};
	#[cfg(feature = "log.scheduler")] debug!("sleeping tid {:?}", sleep_event.tid);
	guard.event_queue.add(sleep_event);
	guard.block(ThreadState::Sleeping);*/
}

pub fn sleep(duration: Duration) {
	// fixme: if duration <= Duration::from_secs(1) {
		// Core pinned sleep
		pinned_sleep(Instant::now() + duration);
	//} else {
	//	todo!();
	//	push_to_global_sleep_queue(Instant::now() + duration);
	//}
}

pub fn sleep_until(wake_time: Instant) {
	// to avoid having to calculate time until wake, always do a pinned sleep and have it pulled from back of queue later
	pinned_sleep(wake_time);
}

pub fn exit(exit_code: i8) -> ! {
	/*let mut guard = scheduler::SCHEDULER.lock();
	guard.queue_for_deletion(exit_code);
	drop(guard); // drop guard before end of scope to ensure deferred schedule goes through */
	todo!();
	unreachable!("Returned to deleted task")
}

#[naked]
pub unsafe extern "C" fn thread_startup() {
	extern "C" fn thread_startup_inner() {
		unsafe { scheduler::scheduler().thread_startup(); }
		debug!("thread_startup");
	}

	naked_asm!(
		".cfi_startproc simple",
		".cfi_def_cfa rsp, 48",
		".cfi_offset rip, -48",
		"pop rbp", // aligns to 16 bytes
		".cfi_def_cfa rsp, 40",
		".cfi_register rip, rbp",
		"call {}",
		"pop rdi", // pop args off stack
		".cfi_def_cfa rsp, 32",
		"pop rsi",
		".cfi_def_cfa rsp, 24",
		"pop rdx",
		".cfi_def_cfa rsp, 16",
		"pop rcx",
		".cfi_def_cfa rsp, 8",
		"ret",
		".cfi_endproc",
	sym thread_startup_inner);
}

pub fn defer_schedule() {
	crate::hal::arch::apic::send_self_ipi(0x30);
}

pub fn debug() { debug!("{:#?}", scheduler::scheduler()); }
