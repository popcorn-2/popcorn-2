use core::sync::atomic::{fence, AtomicPtr, AtomicUsize, Ordering};
use kernel_api::address_space::AddressSpace;
use kernel_api::allocator::AllocError;
use kernel_api::memory::EpochGuard;
use kernel_api::syscall::HandleMap;
use kernel_api::threading::{AtomicThreadState, TaskRef, ThreadState};
use crate::ebr;
#[cfg(feature = "hal-next")] use crate::arch;
#[cfg(not(feature = "hal-next"))] use crate::hal;

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
	#[cfg(feature = "hal-next")] registers: arch::SavedRegisters,
	#[cfg(not(feature = "hal-next"))] registers: hal::SaveState,
	/// List of handles available to this task.
	handles: HandleMap,
	/// Address space used for this task.
	address_space: AddressSpace,
	/// Intrusive collection parts.
	intrusive: Intrusive,
}

impl Task {
	pub fn as_ref(&'static self) -> TaskRef {
		// synchronise with Acquire ordering in TaskRef::get
		let generation = self.intrusive.generation(Ordering::Release);
		// SAFETY: TaskRef is only created from &Task
		unsafe { TaskRef::new(self, generation) }
	}

	fn finalize(&self) {
		warn!("task finalizer unimplemented");
	}

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
		Ok(Self {
			linked_to: None,
			state: AtomicThreadState::new(ThreadState::Killed(0)),
			registers: Default::default(),
			handles: Default::default(),
			address_space: AddressSpace::empty()?,
			intrusive: Default::default(),
		})
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

mod sealed { pub trait Sealed {} }

trait TaskRefExt: sealed::Sealed {
	fn get(self) -> Option<&'static Task>;
}

impl sealed::Sealed for TaskRef {}
impl sealed::Sealed for Option<TaskRef> {}

impl TaskRefExt for TaskRef {
	fn get(self) -> Option<&'static Task> {
		let ptr = self.as_ptr();
		let generation = self.generation();

		// SAFETY: TaskRef is only created from refs to Task
		let task = unsafe { ptr.cast::<Task>().as_ref() };
		// synchronises with Release in Task::alloc, Task::as_ref, and Intrusive::bump
		let actual_generation = task.intrusive.generation.load(Ordering::Acquire);
		(generation == actual_generation).then_some(task)
	}
}

impl TaskRefExt for Option<TaskRef> {
	fn get(self) -> Option<&'static Task> {
		self.and_then(TaskRef::get)
	}
}
