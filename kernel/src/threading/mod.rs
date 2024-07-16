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
use alloc::sync::{Arc, Weak};
use core::arch::{asm, naked_asm};
use core::cmp::Ordering;
use core::fmt::{Debug, Formatter};
use core::{mem, ptr};
use core::mem::ManuallyDrop;
use core::num::{NonZero, NonZeroU16, NonZeroUsize};
use core::ptr::{addr_of, DynMetadata, NonNull};
use core::sync::atomic::AtomicUsize;
use core::time::Duration;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::{highmem, OwnedFrames};
use kernel_api::memory::r#virtual::{Global, OwnedPages};
use kernel_api::sync::IrqCell;
use kernel_api::time::Instant;
use crate::threading::tcb::{ThreadControlBlock, ThreadState};
use crate::hal::{ContextSwitchPreserve, Hal, HalTy, SaveState};
use crate::{non_zero, assert_unsafe_precondition};
use scheduler::Scheduler;
use crate::hal::paging2::{TTable, TTableTy};
use crate::memory::paging::ktable;
use crate::threading::scheduler::GlobalThread;

mod scheduler;
pub mod tcb;
mod cleanup;

const INIT_THREAD_NUM: usize = 1;
const INIT_THREAD_ID: ThreadId = ThreadId { id: non_zero!(INIT_THREAD_NUM) };

/// The numerical ID of a thread
/// 
/// See the [module level documentation](crate::threading#threadcontrolblock-vs-thread-vs-threadpointer-vs-threadid) for more information
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ThreadId {
	id: NonZero<usize>,
}

impl ThreadId {
	/// Creates a new unique [`ThreadId`]
	/// 
	/// The exact semantics of how the numerical ID is determined is implementation defined and should not be relied upon
	/// 
	/// # Examples
	/// 
	/// ```
	/// use kernel::threading::ThreadId;
	/// 
	/// let thread_a = ThreadId::new();
	/// let thread_b = ThreadId::new();
	/// assert_ne!(thread_a, thread_b);
	/// ```
	fn new() -> Self {
		static THREAD_IDS: AtomicUsize = AtomicUsize::new(INIT_THREAD_NUM + 1);

		let id = THREAD_IDS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
		let id = NonZero::<usize>::new(id)
				.expect("`ThreadId` value overflowed");

		ThreadId {
			id
		}
	}
}

/// The numerical ID of a CPU core
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct CoreId {
	id: usize,
}

/// Global ownership of a [`ThreadControlBlock`]
/// 
/// See the [module level documentation](crate::threading#threadcontrolblock-vs-thread-vs-threadpointer-vs-threadid) for more information
pub struct Thread {
	ptr: NonNull<ThreadControlBlock>,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

/// Scheduler ownership of a [`ThreadControlBlock`]
///
/// See the [module level documentation](crate::threading#threadcontrolblock-vs-thread-vs-threadpointer-vs-threadid) for more information
#[repr(transparent)]
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

	/// Immutably "borrows" a [`ThreadPointer`]
	fn tcb_ref(&self) -> tcb::SharedView<'_> {
		let tcb = unsafe { self.ptr.as_ref() };
		tcb::SharedView::from_tcb(tcb)
	}

	/// Mutably "borrows" a [`ThreadPointer`]
	fn tcb_mut(&mut self) -> tcb::PointerView<'_> {
		let tcb = unsafe { self.ptr.as_ref() };
		unsafe { tcb::PointerView::from_tcb(tcb) }
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

/// Initializes the scheduler subsystem
/// 
/// Initializes the scheduler for the bootstrap core, and creates the [`ThreadControlBlock`] for the already running
/// first thread. Using `handoff_data`, this moves ownership of the in-use stack and page tables into the new
/// [`ThreadControlBlock`].
pub fn init(handoff_data: crate::HandoffWrapper) -> (ThreadId, CoreId) {
	let stack = handoff_data.memory.stack;
	let ttable = handoff_data.to_empty_ttable();

	// fixme: is highmem always correct?
	let stack_phys_len = stack.top_virt - stack.bottom_virt - 1;
	let stack_frames = unsafe {
		OwnedFrames::from_raw_parts(
			stack.top_phys - stack_phys_len,
			NonZero::<usize>::new(stack_phys_len).expect("Cannot have a zero sized stack"),
			highmem()
		)
	};
	let stack_pages = unsafe {
		OwnedPages::from_raw_parts(
			stack.bottom_virt,
			NonZero::<usize>::new(stack_phys_len + 1).expect("Cannot have a zero sized stack"),
			Global
		)
	};

	let tcb = ThreadControlBlock::new_inner(
		ttable,
		Default::default(),
		Cow::Borrowed("init"),
		unsafe { Stack::from_contiguous_raw_parts(stack_frames, stack_pages) },
		ThreadState::Running,
		INIT_THREAD_ID,
	);
	let thread = Thread::new(tcb);
	let ptr = ThreadPointer { ptr: thread.ptr };
	assert!(
		scheduler::TASK_LIST.lock()
				.try_insert(INIT_THREAD_ID, GlobalThread::Enqueued(thread))
				.is_ok(),
		"ThreadId(1) should not exist already"
	);
	
	let core = scheduler::create_scheduler_for_current_core(ptr);

	(INIT_THREAD_ID, core)
}

/// Spawns a new thread in its own address space
/// 
/// The thread is created with the passed `name` in a new address space. It will run the passed closure on start,
/// and will exit with a success code if the closure returns. A failure exit code can be returned by explicitly
/// calling [`exit()`].
/// 
/// # Examples
/// 
/// ```
/// use kernel::threading::spawn_with;
/// # use kernel::prelude::debug;
/// 
/// let new_id = spawn_with(|| {
///     debug!("`My new thread` is running!");
/// }, "My new thread".into());
/// 
/// debug!("Spawned a new thread with id {new_id:?}");
/// // TODO: get the exit state of the thread
/// ```
///
/// ```
/// use kernel::threading::{spawn_with, exit};
/// # use kernel::prelude::debug;
///
/// let new_id = spawn_with(|| {
///     error!("Oh no something went wrong");
///     exit(-1);
/// }, "Bad thread".into());
///
/// // TODO: get the exit state of the thread
/// ```
/// 
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

	scheduler::enqueue_new(tcb);

	id
}

#[derive(Debug)]
struct ParkState {
	thread_id: ThreadId,
}

#[derive(Debug)]
pub struct ThreadWaker {
	park_state: Weak<ParkState>,
}

impl ThreadWaker {
	pub fn wake(&self, reason: WakeReason) {
		if let Some(state) = self.park_state.upgrade() {
			// FIXME: race condition between upgrading and actually waking which could cause a spurious wakeup
			let tid = state.thread_id;
			let mut guard = scheduler::TASK_LIST.lock();
			let Some(global_thread) = guard.get_mut(&tid) else {
				warn!("Bad thread id {tid:?}");
				return;
			};
			
			scheduler::enqueue_existing(global_thread, reason);
		}
	}
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub enum WakeReason {
	Timeout,
	Custom(NonZeroU16),
}

pub trait WakeMechanism {
	fn add_waker(&self, waker: ThreadWaker);
}

#[derive(Debug)]
#[non_exhaustive]
pub struct ParkError {}

/// Park the current thread until it is woken up
///
/// This will park the thread until a [`ThreadWaker`] for this park event wakes the thread.
/// The kernel guarantees that this function will not return unless either there is an error
/// when parking the thread, or the thread is woken.
///
/// [`ThreadWaker`]s are generated in `park()`, and added to the [`WakeMechanism`]s passed to `park()`.
/// A [`ThreadWaker`] will only wake a thread if it was generated for the current park event. This means
/// that a [`ThreadWaker`] will only wake a thread once, even if [`wake()`](ThreadWaker::wake) is
/// repeatedly called.
///
/// # Atomicity
///
/// The entirety of a call to `park()` executes atomically - if the thread is woken by a valid [`ThreadWaker`]
/// before `park()` has finished executing, it will act like [`yield_now()`], and a thread will never get stuck
/// waiting for an event that has already occurred.
///
/// This means the following code is guaranteed to always make progress:
/// ```rust
/// use kernel::threading::{park, WakeReason, WakeMechanism};
/// 
/// struct WakeImmediately;
/// 
/// impl WakeMechanism for WakeImmediately {
///     fn add_waker(&self, waker: ThreadWaker) {
///         waker.wake(WakeReason::Custom(non_zero!(1)));
///     }
/// }
///
/// park(&[&WakeImmediately]).unwrap();
/// ```
///
/// # Errors
///
/// # Examples
///
/// ```
/// # use kernel_api::sync::Spinlock;
/// use kernel::threading::{park, ThreadWaker, WakeReason, WakeMechanism};
///
/// struct TimerWaker {
///     waker: Spinlock<Option<ThreadWaker>,
/// }
/// 
/// impl WakeMechanism for TimerWaker {
///     fn add_waker(&self, waker: ThreadWaker) {
///         *self.waker.lock() = Some(waker);
///     }
/// }
/// 
/// static WAKER: TimerWaker = TimerWaker { waker: Spinlock::new(None) };
///
/// // Called periodically by a timer interrupt
/// pub fn periodic_timer_handler() {
///     if let Some(waker) = &mut *WAKER.waker.lock() {
///         waker.wake(WakeReason::Timeout);
///     }
/// }
///
/// fn main() {
///     // This call will return the next time the timer interrupt goes off
///     park(&[&WAKER]).expect("failed to park thread");
///
///     // This call will never return, even if the timer interrupt goes off again, unless
///     // there was an error in `park`
///     park(&[]).expect("failed to park thread");
///     unreachable!();
/// }
/// ```
pub fn park(wake_mechanisms: &[&dyn WakeMechanism]) -> Result<WakeReason, ParkError> {
	let id = current_thread();
	debug!("Parking thread {:?}", id.unwrap());
	let weak_ptr = {
		let mut guard = scheduler::scheduler().lock();
		let thread = guard.current_thread().expect("Cannot park when not running a thread");
		let park_state = Arc::new(ParkState { thread_id: *thread.thread_id });
		let weak_ptr = Arc::downgrade(&park_state);
		*thread.state = ThreadState::Parked(park_state);
		weak_ptr
	};
	// Set the state to `Parked` before calling the closure, so if events are triggered
	// during the closure, the thread already appears parked and will get unparked before yielding
	// Also drop the scheduler lock so that waking doesn't cause a deadlock
	for wake_mechanism in wake_mechanisms {
		wake_mechanism.add_waker(ThreadWaker { park_state: weak_ptr.clone() });
	}
	Ok(
		yield_now().expect("State was set to `Parked` before yielding so must have a reason to wake")
	)
}

/// Gets the [`ThreadId`] for the thread currently running on this core
/// 
/// Returns `None` if the core is idle
pub fn current_thread() -> Option<ThreadId> {
	scheduler::scheduler().lock().current_thread().map(|t| *t.thread_id)
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
	extern "C" fn thread_startup_inner(previous_thread: ThreadPointer) {
		let guard = unsafe { scheduler::scheduler().make_guard_unchecked() };
		debug!("[b] switch from `{:?}` to current", previous_thread.tcb_ref().thread_id,);
		guard.switch_thread_post(previous_thread);
		debug!("thread_startup");
	}

	naked_asm!(
		".cfi_startproc simple",
		".cfi_def_cfa rsp, 48",
		".cfi_offset rip, -48",
		"pop rbp", // aligns to 16 bytes
		".cfi_def_cfa rsp, 40",
		".cfi_register rip, rbp",
		"mov rdi, rax",
		".cfi_undefined rdi",
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

/// Adds a pending thread switch that will switch threads once all nested interrupts are handled.
///
/// This will immediately return regardless of the current thread's blocked state.
///
/// # Interrupt safety
/// This function **is** interrupt safe, and will immediately return
pub fn yield_defer() {
	crate::hal::arch::apic::send_self_ipi(0x30);
}

/// Immediately invokes the scheduler to switch threads.
///
/// If the thread has be placed into a blocked state, this will not return until it is unblocked.
///
/// # Interrupt safety
/// This function is **not** interrupt safe, and will block any pending interrupts
pub fn yield_now() -> Option<WakeReason> {
	// First we lock the scheduler for the current core, and ask it for the current and new threads
	// Wrap it in `ManuallyDrop` since we recreate the guard later, as the thread may have migrated
	// during the context switch
	let mut scheduler = ManuallyDrop::new(scheduler::scheduler().lock());
	let (from, to_view) = scheduler.switch_thread_pre();

	// We need to duplicate the `ThreadPointer` so we can pass it to `switch_thread` while it is borrowed
	// so wrap the first copy in a `ManuallyDrop` to prevent a double free
	let mut from = ManuallyDrop::new(from);
	// Get a pointer to the `ThreadPointer` to use to duplicate it later
	let from_ptr = addr_of!(*from);
	// Extract the `PointerView`
	// The `PointerView` does not borrow the contents of the `ThreadPointer` - it only requires the
	// `ThreadPointer` to exist 'somewhere', so holding this borrow while moving the underlying `ThreadPointer`
	// is safe
	let from_view = from.tcb_mut();

	debug!("[a] switch from `{:?}` to `{:?}`", from_view.thread_id, to_view.thread_id);
	
	assert!(to_view.state.is_ready());
	let reason = to_view.state.wake_reason();
	*to_view.state = ThreadState::Running;
	if from_view.state.is_running() { *from_view.state = ThreadState::Ready; }

	// From the CPU's perspective during a context switch, `from` is no longer the same `ThreadPointer`
	// as the stack has been changed. Instead, we replace it with the `ThreadPointer` that `switch_thread`
	// preserves across the function call
	let ContextSwitchPreserve(mut from, reason) = unsafe { <HalTy as Hal>::switch_thread(&from_view, &to_view, ContextSwitchPreserve(ptr::read(from_ptr), reason)) };

	{
		let tcb = from.tcb_mut();
		debug!("[b] switch from `{:?}` to current, old blocked in state {:?}, new woken due to {reason:?}", tcb.thread_id, tcb.state);
	}

	// Then we pass the old `ThreadPointer` back to the scheduler for it to enqueue
	unsafe { scheduler::scheduler().make_guard_unchecked() }.switch_thread_post(from);

	reason
}

#[doc(hidden)]
pub fn debug() { debug!("{:#?}", scheduler::scheduler()); }
