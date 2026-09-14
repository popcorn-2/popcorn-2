use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use kernel_api::address_space::AddressSpace;
use kernel_api::memory::EpochGuard;
use kernel_api::syscall::HandleMap;
use kernel_api::threading::{AtomicThreadState, TaskRef, ThreadState};
use crate::ebr;
#[cfg(feature = "hal-next")] use crate::arch;
#[cfg(not(feature = "hal-next"))] use crate::hal;

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
		let generation = self.generation();
		TaskRef::new(self, generation)
	}

	fn generation(&self) -> usize {
		self.intrusive.generation(Ordering::Acquire)
	}

	fn finalize(&self) {
		warn!("task finalizer unimplemented");
	}
}

impl Default for Task {
	fn default() -> Self {
		Self {
			linked_to: None,
			state: AtomicThreadState::new(ThreadState::Killed(0)),
			registers: Default::default(),
			handles: Default::default(),
			address_space: Default::default(),
			intrusive: Default::default(),
		}
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
