use core::num::NonZero;
use core::ops::Range;
use core::sync::atomic::{fence, AtomicPtr, AtomicUsize, Ordering};
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

mod scheduler;
mod collections;
mod startup;

pub use scheduler::{RoundRobin as Scheduler, scheduler_entry};
pub use startup::ProcInfo;

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
	linked_to: Option<TaskRef>,
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
	/// Intrusive collection parts.
	intrusive: Intrusive,
}

impl Task {
	pub fn as_ref(&'static self) -> TaskRef {
		// synchronise with Acquire ordering in TaskRef::get
		fence(Ordering::Release);
		let generation = self.intrusive.generation(Ordering::Relaxed);
		// SAFETY: TaskRef is only created from &Task
		unsafe { TaskRef::new(self, generation) }
	}

	fn finalize(&self) {
		warn!("task finalizer unimplemented");
	}

	// must pass task in killed state to `with`, with all other fields in a default/empty state
	pub fn alloc(with: impl FnOnce(&'static Task)) -> Result<OwnedTask, AllocError> {
		let backing = {
			let task = Box::new_uninit_in(alloc::alloc::Global);
			let task = Box::write(task, Task::new()?);
			OwnedTask(Box::leak(task))
		};

		with(&backing.0);
		// synchronises with Acquire ordering in TaskRef::get
		fence(Ordering::Release);
		Ok(backing)
	}

	pub fn dealloc(this: OwnedTask) {
		this.0.finalize();

		if this.0.intrusive.generation.load(Ordering::Acquire) == TaskRef::MAX_GENERATION {
			warn!("task hit maximum generation - leaking");
		} else {
			// FIXME: memory leak - add to free list
		}
	}

	fn new() -> Result<Self, AllocError> {
		let task = Self {
			linked_to: None,
			state: AtomicThreadState::new(ThreadState::Killed(0)),
			registers: Default::default(),
			handles: Default::default(),
			address_space: AddressSpace::empty()?,
			intrusive: Default::default(),
		};

		Ok(task)
	}

	pub fn handles(&self) -> &HandleMap {
		&self.handles
	}
}

#[derive(Debug, Default)]
struct Intrusive {
	generation: AtomicUsize,
	// FIXME(GenericAtomic, @Beanie496): replace with Atomic<Option<TaskRef>>
	next: AtomicPtr<u8>,
	// FIXME(GenericAtomic, @Beanie496): replace with Atomic<Option<TaskRef>>
	prev: AtomicPtr<u8>,
}

impl Intrusive {
	pub fn bump(&self, from: usize) -> Result<usize, usize> {
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
	let mut task = Task::new().expect("failed to create task");

	task.address_space = AddressSpace::from_parts(
		ttable.0,
		LinkedListAllocator::new(Range {
			start: ttable.1,
			end: RawPage::new(0x8000_0000_0000),
		}).expect("failed to create allocator"),
	);

	let task = Task::alloc(move |task| {
		drop(unsafe { task.address_space.swap(address_space, Ordering::Relaxed) });
		task.state.store(ThreadState::Running, Ordering::Relaxed)
	}).expect("failed to allocate task");

	percpu!(current_task).set(Some(task.0.as_ref()));
	task.0
}

mod sealed { pub trait Sealed {} }

pub trait TaskRefExt: sealed::Sealed {
	fn get(self) -> Option<&'static Task>;
	fn get_unchecked(self) -> (&'static Task, usize);
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
		// synchronises with Release in Task::alloc, Task::as_ref, and Intrusive::bump
		let actual_generation = task.intrusive.generation.load(Ordering::Acquire);
		(task, actual_generation)
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
}
