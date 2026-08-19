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

use alloc::sync::Arc;
use core::convert::Into;
use core::fmt::Debug;
use core::num::NonZero;
use core::ops::Range;
use core::ptr;
use core::sync::atomic::Ordering;
use futures::task::AtomicWaker;
use scheduler::Scheduler;
use kernel_api::address_space::AddressSpace;
use kernel_api::allocator::{highmem, AllocError};
use kernel_api::mapping::{Caching, Config, Mapping, Protection, Ty};
use kernel_api::memory::{Frames, RawPage};
use kernel_api::threading::{AtomicThreadState, ThreadId, ThreadMeta, ThreadState};
use linked_list_allocator::LinkedListAllocator;
use kernel_api::syscall::handle::HandleMap;
use kernel_api::sync::LazyLock;
use crate::hal::paging2::TTable;
use crate::hal::{SaveState, TTableTy};
use crate::memory::paging::ktable;
use kernel_api::syscall::AsyncMap;

//mod cleanup;
mod parking;
//mod pointers;
mod scheduler;
mod sleeping;
//mod thread_control_block;
mod yielding;
//mod subthread_killer;
mod tcb;

#[allow(unused_imports)]
mod export {
	pub use super::sleeping::sleep_until;
	pub use super::tcb::ThreadControlBlock;
	pub use super::yielding::{yield_now, post_switch_cleanup};
}
#[allow(unused_imports)] pub use export::*;

pub type SchedulerTy = impl Scheduler;

const INIT_THREAD_ID: ThreadId = ThreadId::new(0);

/// The numerical ID of a CPU core
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct CoreId {
	id: isize,
}

#[unsafe(export_name = "__popcorn_threading_modify_current_thread_meta")]
fn with_current_thread(arg: *mut (), f: fn(&Arc<ThreadMeta>, *mut ())) {
	let guard = percpu_v2!(current_thread).read();
	let tcb = guard.as_ref().expect("cannot call `with_current_thread` from idle");
	f(&tcb.meta, arg)
}

#[unsafe(export_name = "__popcorn_threading_unblock_thread")]
fn unblock_thread(this: &Arc<ThreadMeta>) {
	// fixme: smp
	scheduler::local_scheduler()
			.lock()
			.unpark(this.thread_id);
}

/// Initializes the scheduler subsystem
/// 
/// Initializes the scheduler for the bootstrap core, and creates the [`ThreadControlBlock`] for the already running
/// first thread. Using `handoff_data`, this moves ownership of the in-use stack and page tables into the new
/// [`ThreadControlBlock`].
pub fn init(stack: utils::handoff::Stack, ttable: TTableTy) -> (ThreadId, CoreId) {
	let address_space = AddressSpace::from_parts(
		ttable,
		LinkedListAllocator::new(Range { // todo: make this a bit nicer
			start: RawPage::new(0x200000),
			end: RawPage::new(0x8000_0000_0000),
		}).expect("failed to create allocator"),
	);

	// fixme: is highmem always correct?
	let stack_frames = unsafe {
		Frames::<true>::from_raw(Range {
			start: stack.bottom_phys,
			end: stack.bottom_phys + stack.page_count,
		}, highmem())
	};

	let stack = unsafe {
		Mapping::from_raw_parts(
			stack_frames,
			stack.bottom_virt - 1,
			Protection { executable: false, writable: true, user_accessible: false },
			Caching::Normal,
		)
	};

	let meta = ThreadMeta {
		name: "initd".into(),
		kernel_stack: stack,
		state: AtomicThreadState::new(ThreadState::Running),
		thread_id: INIT_THREAD_ID,
		address_space,
		handles: HandleMap::new(),
		async_map: Arc::new(AsyncMap::new()),
		join_waiter: AtomicWaker::new(),
	};

	let tcb = ThreadControlBlock {
		register_state: SaveState::default(),
		meta: Arc::new(meta),
	};

	crate::ipc::init_proc(Arc::clone(&tcb.meta));
	percpu_v2!(kernel_stack_top).store(tcb.kernel_stack.as_ptr_range().end.cast_mut(), Ordering::Relaxed);
	crate::hal::first_thread_init(&tcb);
	*percpu_v2!(current_thread).write() = Some(tcb);

	let core = scheduler::create_scheduler_for_current_core();
	
	crate::hal::enable_interrupts();

	(INIT_THREAD_ID, core)
}

static KERNEL_ADDRESS_SPACE: LazyLock<AddressSpace> = LazyLock::new(|| AddressSpace::from_parts(
	TTableTy::new(&ktable(), highmem()).expect("failed to allocate kernel address space page table"),
	LinkedListAllocator::new(RawPage::new(0)..RawPage::new(0)).expect("failed to allocate kernel address space page table"),
));

const KERNEL_THREAD_STACK: Config = Config::new(NonZero::new(32).unwrap(), Ty::KERNEL_STACK)
		.protection(true, false, false);

/// Spawns a new kernel worker thread
/// 
/// The thread is created with the passed `name` and a TID of -1.
/// It will not appear in any proc server output, and therefore will be invisible to userspace.
/// It will not have any lower half address space, and any attempts to create a lower half [`Mapping`] will
/// error.
/// It will run the passed closure on start, and will exit with code 0 if the closure returns normally.
/// A failure exit code can be returned by explicitly calling [`exit()`].
/// Any panics will be treated as a kernel panic, and will halt the entire system.
///
/// On success, an (`Arc<ThreadMeta>`)[ThreadMeta] object will be returned, which can be used to query
/// the current thread state.
/// The return code can be retrieved by calling [`ThreadMeta::join()`].
///
/// # Errors
///
/// [`AllocError`] is returned if any memory allocations failed while creating the thread.
/// 
/// # Examples
/// 
/// ```
/// use kernel::threading::spawn_kernel;
/// use kernel_api::executor::block_on;
/// # use kernel::prelude::*;
///
/// let meta = spawn_kernel("My new thread".into(), || {
///     debug!("`My new thread` is running!");
/// }).unwrap();
///
/// let exit_code = block_on(meta.join());
/// assert_eq!(exit_code, 0);
/// ```
///
/// ```
/// use kernel::threading::{spawn_kernel, exit};
/// use kernel_api::executor::block_on;
/// # use kernel::prelude::*;
///
/// let meta = spawn_kernel("My new thread".into(), || {
///     exit(-1);
/// }).unwrap();
///
/// let exit_code = block_on(meta.join());
/// assert_eq!(exit_code, -1);
/// ```
pub fn spawn_kernel(name: Arc<str>, f: impl FnOnce() + Send + 'static) -> Result<Arc<ThreadMeta>, AllocError> {
	// todo: make different threads have different TIDs
	let meta = spawn(
		name,
		AddressSpace::clone(&*KERNEL_ADDRESS_SPACE),
		ThreadId::new(-1),
		HandleMap::new(),
		Arc::new(AsyncMap::new()),
		f,
	)?;

	Ok(meta)
}

/// Insert a new thread into the scheduler infrastructure
pub fn spawn(name: Arc<str>, address_space: AddressSpace, thread_id: ThreadId, handles: HandleMap, async_map: Arc<AsyncMap>, f: impl FnOnce() + Send + 'static) -> Result<Arc<ThreadMeta>, AllocError> {
	let meta = ThreadMeta {
		name,
		kernel_stack: KERNEL_THREAD_STACK.map()?,
		state: AtomicThreadState::new(ThreadState::Ready),
		thread_id,
		address_space,
		handles,
		async_map,
		join_waiter: AtomicWaker::new(),
	};

	extern "C" fn main(ptr: usize) -> ! {
		exit_trampoline(move || -> ! {
			let boxed = unsafe { Box::<Box<dyn FnOnce()>>::from_raw(ptr::with_exposed_provenance_mut(ptr)) };
			boxed();
			exit(0);
		})
	}

	let boxed = Box::new(f) as Box<dyn FnOnce()>;
	let boxed = Box::into_raw(Box::new(boxed));

	let (tcb, meta) = ThreadControlBlock::new(
		meta,
		main,
		boxed.expose_provenance()
	)?;

	scheduler::enqueue(tcb);

	Ok(meta)
}

struct TerminateThread(isize);

pub fn exit(exit_code: isize) -> ! {
	if exit_code != 0 {
		warn!("Exit thread with code {exit_code}");
	} else {
		debug!("Exit thread with code {exit_code}");
	}

	// todo: do we need to store the exit code in the panic?
	crate::panicking::do_panic_with(Box::new(TerminateThread(exit_code)));
}

/// Calls the passed function, catching any `TerminateThread` panics
/// 
/// This must be called at all kernel entrypoints to properly terminate a thread.
/// Not calling this will result in a kernel panic on kernel exit.
pub fn exit_trampoline<R, F: FnOnce() -> R + core::panic::UnwindSafe>(f: F) -> R {
	crate::panicking::catch_unwind(f).unwrap_or_else(|e| {
		if let Some(TerminateThread(exit_code)) = e.downcast_ref() {
			if *exit_code != 0 {
				info!("Unwound thread for exit code {exit_code}");
			} else {
				debug!("Unwound thread for exit code {exit_code}");
			}

			{
				let guard = percpu_v2!(current_thread).read();
				let guard = guard.as_ref().expect("cannot exit from idle thread");
				guard.state.store(ThreadState::Killed(*exit_code), Ordering::SeqCst);
				guard.join_waiter.wake();
			}
			yield_now();

			unreachable!("Failed to exit thread");
		} else {
			crate::panicking::resume_unwind(e)
		}
	})
}

#[doc(hidden)]
pub fn debug() {
	let thread = percpu_v2!(current_thread)
			.read();
	
	info!("current_thread = {thread:?}\n\n{:#?}", scheduler::local_scheduler());
}
