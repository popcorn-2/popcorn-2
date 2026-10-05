use core::num::NonZero;
use core::ops::Range;
use core::pin::Pin;
use core::sync::atomic::{fence, AtomicPtr, AtomicUsize, Ordering, AtomicBool};
use kernel_api::address_space::AddressSpace;
use kernel_api::allocator::AllocError;
use kernel_api::mapping::{Config, Mmap, Ty};
use kernel_api::memory::RawPage;
use kernel_api::syscall::HandleMap;
use kernel_api::threading::{AtomicThreadState, TaskRef, ThreadState};
use linked_list_allocator::LinkedListAllocator;
use crate::hal::TTableTy;
use crate::{arch, percpu};
use crate::memory::r#virtual::AddressSpaceExt;
use kernel_api::sync::Spinlock;
use crate::task::collections::{PopResult, SinglyLinkedList};

mod scheduler;
mod collections;
mod idle;
mod startup;
mod wait_queue;
mod futex;
mod exiting;

pub use scheduler::{RoundRobin as Scheduler, scheduler_entry};
pub use startup::ProcInfo;
pub use wait_queue::WaitQueue;
pub use futex::{futex_wait, futex_wake};
pub use exiting::{kill_task, detach_task, wait_task};

static FREE_TASK_LIST: Spinlock<SinglyLinkedList> = Spinlock::new(SinglyLinkedList::new());

#[derive(Debug)]
pub struct OwnedTask(&'static Task);

#[derive(Debug)]
pub struct Task {
	/// Current state of the task.
	// FIXME(GenericAtomic, @Beanie496): replace with Atomic<ThreadState>
	state: AtomicThreadState,
	/// Associated task.
	///
	/// Use of this field depends on the current thread state:
	/// * [`ThreadState::Ready`] and [`ThreadState::Running`] - the task that this
	///   task has yielded it's timeslice to.
	/// * All other states - currently unused.
	// FIXME(GenericAtomic, @Beanie496): replace with Atomic<Option<TaskRef>>
	linked_to: AtomicPtr<u8>,
	/// This is a backlink of the [`linked_to`](Task.linked_to) field.
	// FIXME(GenericAtomic, @Beanie496): replace with Atomic<Option<TaskRef>>
	linked_from: AtomicPtr<u8>,
	/// Saved register state of the task.
	///
	/// <div class="warning">
	/// This will only be completely up-to-date if the task is not actively running,
	/// and was last switched away from due to a full context switch rather than an
	/// IPC call.
	/// </div>
	pub registers: arch::SavedRegisters,
	/// List of handles available to this task.
	handles: HandleMap,
	/// Address space used for this task.
	pub address_space: AddressSpace,
	syscall_trampoline: AtomicPtr<u8>,
	/// Intrusive collection parts.
	intrusive: Intrusive,
	exit_queue: WaitQueue,
	detached: AtomicBool,
}

impl Task {
	pub fn as_ref(&'static self) -> TaskRef {
		let generation = self.intrusive.generation(Ordering::Relaxed);
		// SAFETY: TaskRef is only created from &Task
		unsafe { TaskRef::new(self, generation) }
	}

	pub fn exit_queue(&'static self) -> Pin<&WaitQueue> {
		// SAFETY: `Task` is pinned and heap-allocated at a stable address.
		unsafe { Pin::new_unchecked(&self.exit_queue) }
	}

	fn finalize(&'static self) {
		core::debug_assert_matches!(
			self.state.load(Ordering::Acquire),
			ThreadState::Zombie(_),
			"only zombie threads should be finalised",
		);

		let queue = self.intrusive.blocked_on.load(Ordering::Acquire);
		// SAFETY: `queue_ptr` was stored via `wait_with` from a valid `WaitQueue` reference.
		//  The `WaitQueue` remains valid while tasks are enqueued on it.
		if let Some(queue) = unsafe { queue.as_ref() } {
			queue.remove_task(self);
		}

		self.detached.store(false, Ordering::Relaxed);

		warn!("task finalizer unimplemented");
	}

	// must pass task in killed state to `with`, with other fields in a default/empty state
	pub fn alloc(address_space: AddressSpace, with: impl FnOnce(&'static Task)) -> Result<OwnedTask, AllocError> {
		let backing = {
			let backing = match FREE_TASK_LIST.lock().pop_front() {
				PopResult::Some(task) => Some(task),
				PopResult::None => None,
				PopResult::Outdated(_) => unreachable!("tasks already in free list should not get invalidated")
			};
			backing.unwrap_or_else(|| {
				let task = Box::new_uninit_in(alloc::alloc::Global);
				let task = Box::write(task, Task::new(address_space));
				OwnedTask(Box::leak(task))
			})
		};

		let syscall_trampoline_map = Config::new(const { NonZero::new(1).unwrap() }, Ty::USER_PACKET_BUFFER)
			.protection(true, false, true)
			.map_in::<Mmap>("syscall_trampoline".into(), &backing.0.address_space)?;
		let syscall_trampoline_user = syscall_trampoline_map.mapping.as_ptr().addr();
		// SAFETY: syscall_trampoline is always a single real page
		let syscall_trampoline = unsafe { syscall_trampoline_map.mapping.physical_start().unwrap_unchecked() };
		backing.0.syscall_trampoline.store(syscall_trampoline.to_virtual().as_ptr(), Ordering::Relaxed);
		backing.0.registers.set_syscall_trampoline(syscall_trampoline_user);
		drop(syscall_trampoline_map);

		with(&backing.0);
		// synchronises with Acquire ordering in TaskRef::get
		backing.0.intrusive.generation.fetch_add(1, Ordering::Release);
		Ok(backing)
	}

	fn try_dealloc(&'static self) {
		if !self.detached.load(Ordering::Acquire)
			|| !matches!(self.state.load(Ordering::Acquire), ThreadState::Zombie(_)) {
			return;
		}

		self.finalize();

		if self.intrusive.generation.load(Ordering::Acquire) == TaskRef::MAX_GENERATION {
			warn!("task hit maximum generation - leaking");
		} else {
			// deatched and in zombie state, therefore not owned by any runqueues or references
			FREE_TASK_LIST.lock().push_back(OwnedTask(self));
		}
	}

	fn new(address_space: AddressSpace) -> Self {
		Self {
			linked_to: AtomicPtr::new(core::ptr::null_mut()),
			linked_from: AtomicPtr::new(core::ptr::null_mut()),
			state: AtomicThreadState::new(ThreadState::Killed(0)),
			registers: Default::default(),
			handles: Default::default(),
			address_space,
			syscall_trampoline: AtomicPtr::new(core::ptr::null_mut()),
			intrusive: Default::default(),
			exit_queue: WaitQueue::new(),
			detached: AtomicBool::new(false),
		}
	}

	pub fn handles(&self) -> &HandleMap {
		&self.handles
	}

	pub fn syscall_trampoline_page(&self) -> *mut u8 {
		self.syscall_trampoline.load(Ordering::Relaxed)
	}
}

#[derive(Debug, Default)]
struct Intrusive {
	generation: AtomicUsize,
	// FIXME(GenericAtomic, @Beanie496): replace with Atomic<Option<TaskRef>>
	next: AtomicPtr<u8>,
	// FIXME(GenericAtomic, @Beanie496): replace with Atomic<Option<TaskRef>>
	prev: AtomicPtr<u8>,
	blocked_on: AtomicPtr<WaitQueue>,
}

impl Intrusive {
	pub fn bump(&self, from: usize) -> Result<usize, usize> {
		// synchronises with Acquire ordering in TaskRef::get
		self.generation.compare_exchange(
			from,
			from + 1,
			Ordering::AcqRel,
			Ordering::Relaxed,
		)
	}

	pub fn generation(&self, ordering: Ordering) -> usize {
		self.generation.load(ordering) & TaskRef::MAX_GENERATION
	}
}

pub fn init(ttable: (TTableTy, RawPage)) -> &'static Task {
	let address_space = AddressSpace::from_parts(
		ttable.0,
		LinkedListAllocator::new(Range {
			start: ttable.1,
			end: RawPage::new(0x8000_0000_0000),
		}).expect("failed to create allocator"),
	);

	let task = Task::alloc(address_space, move |task| {
		task.state.store(ThreadState::Running, Ordering::Relaxed)
	}).expect("failed to allocate task");

	percpu!(current_task).set(Some(task.0.as_ref()));
	task.0
}

pub fn enqueue(task: OwnedTask) {
	// todo(SMP): prioritise previously used core
	percpu!(scheduler).enqueue(task);
}

mod sealed { pub trait Sealed {} }

pub trait TaskRefExt: sealed::Sealed {
	fn get(self) -> Option<&'static Task>;
	fn get_unchecked(self) -> (&'static Task, usize);
	fn get_or_dead(self) -> Option<(&'static Task, bool)>;
}

impl sealed::Sealed for TaskRef {}
impl sealed::Sealed for Option<TaskRef> {}

impl TaskRefExt for TaskRef {
	fn get(self) -> Option<&'static Task> {
		let generation = self.generation();
		let (task, actual_generation) = self.get_unchecked();
		(generation == actual_generation).then_some(task)
	}

	fn get_unchecked(self) -> (&'static Task, usize) {
		let ptr = self.as_ptr();

		// SAFETY: TaskRef is only created from refs to Task
		let task = unsafe { ptr.cast::<Task>().as_ref() };
		// synchronises with Release in Task::alloc and Intrusive::bump
		let actual_generation = task.intrusive.generation.load(Ordering::Acquire);
		(task, actual_generation)
	}

	fn get_or_dead(self) -> Option<(&'static Task, bool)> {
		let (task, actual_gen) = self.get_unchecked();
		let expected_gen = self.generation();

		// Valid if still in expected_gen (running) or expected_gen + 1 (exited)
		let valid = actual_gen == expected_gen;
		let dead = actual_gen == expected_gen + 1;
		(valid || dead).then_some((task, dead))
	}
}

impl TaskRefExt for Option<TaskRef> {
	fn get(self) -> Option<&'static Task> {
		self.and_then(TaskRef::get)
	}

	fn get_unchecked(self) -> (&'static Task, usize) {
		match self {
			Some(task) => task.get_unchecked(),
			None => unreachable!("should not call get_unchecked on None optional taskref"),
		}
	}

	fn get_or_dead(self) -> Option<(&'static Task, bool)> { self.and_then(TaskRef::get_or_dead) }
}
