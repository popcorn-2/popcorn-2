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
use alloc::sync::Arc;
use core::arch::{asm, naked_asm};
use core::fmt::Debug;
use core::num::NonZero;
use core::ops::{Deref, DerefMut, Range};
use core::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use hashbrown::HashMap;
use kernel_api::memory::{AllocError, Page, VirtualAddress};
use kernel_api::memory::mapping::{Protection, RawStack, Stack};
use kernel_api::memory::physical::{highmem, OwnedFrames};
use kernel_api::memory::r#virtual::{Kernel, OwnedPages};
use kernel_api::sync::{LazyLock, OnceLock, Spinlock, SpinlockGuard};
use crate::{hashmap_new, non_zero};
use scheduler::Scheduler;
use crate::hal::SaveStateTr;

mod cleanup;
mod parking;
mod pointers;
mod scheduler;
mod sleeping;
mod thread_control_block;
mod yielding;
mod subthread_killer;

pub use parking::{park, maybe_park, ParkError, Waker, WakeReason, WakeTrigger};
pub use pointers::{Thread, ThreadPointer};
pub use scheduler::ControlEvent;
use ranged_btree_allocator::RangedBtreeAllocator;
pub use sleeping::{sleep, sleep_until};
pub use thread_control_block::{OwnedView, PointerView, SharedView, ThreadControlBlock, ThreadState};
pub use yielding::{create_idle_thread, yield_defer, yield_now};
use crate::hal::paging2::TTable;
use crate::hal::TTableTy;
use crate::ipc::handle::HandleMap;
use crate::memory::r#virtual::AddressSpaceInner;
use crate::threading::parking::ParkGaurd;
use crate::threading::subthread_killer::SubthreadKiller;
use crate::threading::thread_control_block::AtomicThreadState;

pub type SchedulerTy = impl Scheduler;

const INIT_THREAD_NUM: isize = 1;
const INIT_THREAD_ID: ThreadId = ThreadId { id: non_zero!(INIT_THREAD_NUM) };

/// Global list of all running threads
static TASK_LIST: Spinlock<HashMap<ThreadId, (Thread, PointerState)>> = Spinlock::new(hashmap_new!());

static THREAD_REAPER: LazyLock<ThreadId> = LazyLock::new(|| spawn_kernel(||
	loop {
		debug!("thread reaper loop running");
		let mut guard = TASK_LIST.lock();
		let iter = guard.extract_if(|_, (thread, pointer_state)| {
			matches!(thread.tcb_ref().state.load(Ordering::SeqCst), ThreadState::Dead)
			&& matches!(pointer_state, PointerState::GloballyParked(_))
		}).collect::<Vec<_>>();
		drop(guard);
		for (_, (thread, pointer)) in iter {
			debug!("killing {:?}", thread.tcb_ref().thread_id);
			let PointerState::GloballyParked(pointer) = pointer else { unreachable!("just filtered for globally parked threads") };
			drop(ThreadControlBlock::from_owned(thread, pointer));
		}
		let _ = park(&[]);
	}, Cow::Borrowed("thread reaper")).expect("could not spawn thread reaper")
);

#[derive(Debug)]
enum PointerState {
	InScheduler(CoreId),
	GloballyParked(ThreadPointer),
}

/// The numerical ID of a thread
/// 
/// See the [module level documentation](crate::threading#threadcontrolblock-vs-thread-vs-threadpointer-vs-threadid) for more information
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ThreadId {
	id: NonZero<isize>,
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
		static THREAD_IDS: AtomicIsize = AtomicIsize::new(INIT_THREAD_NUM + 1);

		let id = THREAD_IDS.fetch_add(1, Ordering::Relaxed);
		if id < 0 {
			THREAD_IDS.store(isize::MIN, Ordering::Relaxed);
			panic!("`ThreadId` value overflowed");
		}
		let id = NonZero::<isize>::new(id)
				.expect("`ThreadId` value overflowed");

		ThreadId {
			id
		}
	}
	
	pub fn new_from(val: NonZero<isize>) -> Self {
		Self {
			id: val,
		}
	}
	
	pub fn get(self) -> isize {
		self.id.get()
	}
}

/// The numerical ID of a CPU core
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct CoreId {
	id: isize,
}

/// Initializes the scheduler subsystem
/// 
/// Initializes the scheduler for the bootstrap core, and creates the [`ThreadControlBlock`] for the already running
/// first thread. Using `handoff_data`, this moves ownership of the in-use stack and page tables into the new
/// [`ThreadControlBlock`].
pub fn init(handoff_data: crate::HandoffWrapper) -> (ThreadId, CoreId) {
	let stack = handoff_data.memory.stack;

	let address_space = AddressSpaceInner::new(
		handoff_data.to_empty_ttable(),
		RangedBtreeAllocator::new(Range { // todo: make this a bit nicer
			start: Page::new(VirtualAddress::new(0x200000)),
			end: Page::new(VirtualAddress::new(0x8000_0000_0000)),
		}),
	);

	// fixme: is highmem always correct?
	let stack_phys_len = stack.top_virt - stack.bottom_virt - 1;
	let stack_frames = unsafe {
		OwnedFrames::from_raw_parts(
			stack.top_phys - stack_phys_len,
			NonZero::<usize>::new(stack_phys_len).expect("Cannot have a zero sized stack"),
			highmem(),
		)
	};
	let stack_pages = unsafe {
		OwnedPages::from_raw_parts(
			stack.bottom_virt,
			NonZero::<usize>::new(stack_phys_len + 1).expect("Cannot have a zero sized stack"),
			Kernel,
		)
	};

	let tcb = ThreadControlBlock::new_inner(
		address_space,
		Default::default(),
		Cow::Borrowed("init"),
		unsafe { Stack::from_contiguous_raw_parts(stack_frames, stack_pages, Protection::RWX, RawStack) }, // FIXME: this should only be RW but that doesn't exist
		AtomicThreadState::new(ThreadState::Running),
		INIT_THREAD_ID,
		Arc::new(HandleMap::new()),
		SubthreadKiller::new(INIT_THREAD_ID),
	);
	percpu_v2!(kernel_stack_top).store(tcb.kernel_stack.virtual_valid_end().start().addr, Ordering::Relaxed);
	crate::hal::first_thread_init(&tcb);
	let (thread, ptr) = ThreadPointer::new(Thread::new(tcb));

	let core = scheduler::create_scheduler_for_current_core();
	
	assert!(
		TASK_LIST.lock()
				.try_insert(INIT_THREAD_ID, (thread, PointerState::InScheduler(core)))
				.is_ok(),
		"ThreadId(1) should not exist already"
	);
	
	*percpu_v2!(current_thread).write() = Some(ptr);
	
	crate::hal::enable_interrupts();

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
/// use kernel::threading::spawn_kernel;
/// # use kernel::prelude::debug;
/// 
/// let new_id = spawn_kernel(|| {
///     debug!("`My new thread` is running!");
/// }, "My new thread".into());
/// 
/// debug!("Spawned a new thread with id {new_id:?}");
/// // TODO: get the exit state of the thread
/// ```
///
/// ```
/// use kernel::threading::{spawn_kernel, exit};
/// # use kernel::prelude::debug;
///
/// let new_id = spawn_kernel(|| {
///     error!("Oh no something went wrong");
///     exit(-1);
/// }, "Bad thread".into());
///
/// // TODO: get the exit state of the thread
/// ```
/// 
pub fn spawn_kernel(f: impl FnOnce() + Send + 'static, name: Cow<'static, str>) -> Result<ThreadId, AllocError> {
	let address_space = AddressSpaceInner::empty()?;
	spawn_into(f, name, address_space, HandleMap::new())
}

pub fn clone_current(f: impl FnOnce() + Send + 'static, name: Cow<'static, str>) -> Result<ThreadId, AllocError> {
	extern "C" fn main(ptr: usize) -> ! {
		let boxed = unsafe { Box::<Box<dyn FnOnce()>>::from_raw(ptr as *mut _) };
		boxed();
		exit(0);
	}

	let boxed = Box::new(f) as Box<dyn FnOnce()>;
	let boxed = Box::into_raw(Box::new(boxed));
	
	let (tcb, id) = ThreadControlBlock::clone_from(
		percpu_v2!(current_thread).read().as_ref()
		                          .expect("cannot clone non-existent thread")
		                          .tcb_ref(),
		name,
		thread_startup,
		main,
		boxed as usize,
	);

	let (thread, ptr) = ThreadPointer::new(Thread::new(tcb));

	let mut guard = TASK_LIST.lock();
	let thread = guard.try_insert(id, (thread, PointerState::GloballyParked(ptr)))
	                  .expect("ThreadId reuse");

	scheduler::enqueue(&mut thread.1);

	Ok(id)
}

pub fn spawn_into(f: impl FnOnce() + Send + 'static, name: Cow<'static, str>, address_space: Arc<AddressSpaceInner>, handles: HandleMap) -> Result<ThreadId, AllocError> {
	extern "C" fn main(ptr: usize) -> ! {
		let boxed = unsafe { Box::<Box<dyn FnOnce()>>::from_raw(ptr as *mut _) };
		boxed();
		exit(0);
	}

	let boxed = Box::new(f) as Box<dyn FnOnce()>;
	let boxed = Box::into_raw(Box::new(boxed));

	let (tcb, id) = ThreadControlBlock::new(
		name,
		address_space,
		handles,
		thread_startup,
		main,
		boxed.addr(),
	);

	let (thread, ptr) = ThreadPointer::new(Thread::new(tcb));

	let mut guard = TASK_LIST.lock();
	let thread = guard.try_insert(id, (thread, PointerState::GloballyParked(ptr)))
	                  .expect("ThreadId reuse");

	scheduler::enqueue(&mut thread.1);

	Ok(id)
}

/// Gets the [`ThreadId`] for the thread currently running on this core
/// 
/// Returns `None` if the core is idle
pub fn current_thread() -> Option<ThreadId> {
	percpu_v2!(current_thread).read().as_ref().map(|t| *t.tcb_ref().thread_id)
}

pub fn get_thread(id: ThreadId) -> Option<impl DerefMut<Target = Thread>> {
	let guard = TASK_LIST.lock();
	if guard.get(&id).is_none() { return None; }
	Some(SpinlockGuard::map(
		guard,
		|val| &mut val.get_mut(&id).expect("just checked this exists").0
	))
}

pub struct ThreadPointerGuard<'a> {
	pointer: *mut ThreadPointer,
	guard: SpinlockGuard<'a, HashMap<ThreadId, (Thread, PointerState)>>,
}

impl<'a> ThreadPointerGuard<'a> {
	fn try_new(mut guard: SpinlockGuard<'a, HashMap<ThreadId, (Thread, PointerState)>>, id: ThreadId) -> Option<Self> {
		let val = match &mut guard.get_mut(&id).expect("thread pointer does not exist").1 {
			PointerState::InScheduler(_) => None,
			PointerState::GloballyParked(ptr) => Some(ptr)
		};
		Some(ThreadPointerGuard {
			pointer: val?,
			guard
		})
	}
}

impl Deref for ThreadPointerGuard<'_> {
	type Target = ThreadPointer;

	fn deref(&self) -> &Self::Target {
		unsafe { &*self.pointer }
	}
}

impl DerefMut for ThreadPointerGuard<'_> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { &mut *self.pointer }
	}
}

pub fn try_get_thread_pointer(id: ThreadId) -> Option<ThreadPointerGuard<'static>> {
	let guard = TASK_LIST.lock();
	if guard.get(&id).is_none() { return None; }
	ThreadPointerGuard::try_new(guard, id)
}

fn move_to_global_parking_lot(thread: ThreadPointer) {
	assert!(
		matches!(thread.tcb_ref().state.load(Ordering::SeqCst), ThreadState::Parked(_) | ThreadState::Dead),
		"Cannot place unparked thread in parking lot"
	);

	debug!("Move {thread:?} to global parking lot");

	let mut guard = TASK_LIST.lock();
	let global_thread = guard.get_mut(thread.tcb_ref().thread_id)
	                         .expect("Cannot park a non-existent thread");

	match global_thread.1 {
		PointerState::InScheduler(_) => {
			global_thread.1 = PointerState::GloballyParked(thread);
		},
		PointerState::GloballyParked(_) => {
			unreachable!(
				"Cannot place a thread into the global parking lot if it is already there.\
				Also how are there two `ThreadPointer`s to the same thread?"
			);
		},
	};
	
	if matches!(global_thread.0.tcb_ref().state.load(Ordering::SeqCst), ThreadState::Dead) {
		drop(guard);
		parking::do_wake(*THREAD_REAPER, WakeReason::Custom(non_zero!(1)));
	}
}

pub fn exit(_exit_code: i8) -> ! {
	debug!("Exit thread with code {_exit_code}");

	percpu_v2!(current_thread).read().as_ref().expect("cannot exit from idle thread").tcb_ref()
			.state.store(ThreadState::Dead, Ordering::SeqCst);
	yield_now();
	
	unreachable!("Failed to exit thread");
}

pub fn kill(thread: ThreadId) {
	let mut guard = TASK_LIST.lock();
	let Some(global_thread) = guard.get_mut(&thread) else {
		warn!("Bad thread id {thread:?}");
		return;
	};

	match global_thread.1 {
		PointerState::InScheduler(core_id) => {
			scheduler::send_control_event(core_id, ControlEvent::Kill(thread));
		},
		PointerState::GloballyParked(ref mut ptr) => {
			global_thread.0.tcb_ref().state.store(ThreadState::Dead, Ordering::SeqCst);
			drop(guard);
			parking::do_wake(*THREAD_REAPER, WakeReason::Custom(non_zero!(1)));
		},
	};
}

#[unsafe(naked)]
pub unsafe extern "C" fn thread_startup() {
	naked_asm!(
		".cfi_startproc simple",
		".cfi_def_cfa rsp, 32",
		".cfi_offset rip, -32",
		"pop rbp", // aligns to 16 bytes
		".cfi_def_cfa rsp, 24",
		".cfi_register rip, rbp",
		"mov rdi, rax",
		".cfi_undefined rdi",
		"call {}",
		"pop rdi", // pop args off stack
		".cfi_def_cfa rsp, 16",
		"pop rdi",
		".cfi_def_cfa rsp, 8",
		"ret",
		".cfi_endproc",
	sym yielding::post_switch_cleanup);
}

#[doc(hidden)]
pub fn debug() {
	info!("current_thread = {:?}\n{:#?}\n\n{:#?}", *percpu_v2!(current_thread).read(), TASK_LIST, scheduler::local_scheduler().0);
}
