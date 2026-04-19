use core::fmt::{Debug, Formatter};
use core::ptr::addr_of;
use core::sync::atomic::{Ordering, AtomicU16};

fn into_raw(state: ThreadState) -> u16 {
	let tag = unsafe { *addr_of!(state).cast::<u8>() } as u16;

	match state {
		ThreadState::Killed(exit_code) => {
			let upper = exit_code as u16;
			(upper << 8) | tag
		}
		_ => tag,
	}
}

fn from_raw(raw: u16) -> ThreadState {
	let tag = raw as u8;

	match tag {
		0 => ThreadState::Ready,
		1 => ThreadState::Running,
		2 => ThreadState::Parked,
		3 => ThreadState::NearlyParked,
		4 => ThreadState::Killed((raw >> 8) as i8),
		_ => unreachable!("invalid thread state"),
	}
}

pub struct AtomicThreadState(AtomicU16);

impl AtomicThreadState {
	pub fn new(state: ThreadState) -> Self {
		AtomicThreadState(AtomicU16::new(into_raw(state)))
	}

	pub fn store(&self, state: ThreadState, ordering: Ordering) {
		self.0.store(into_raw(state), ordering);
	}

	pub fn load(&self, ordering: Ordering) -> ThreadState{
		from_raw(self.0.load(ordering))
	}

	pub fn compare_exchange(&self, current: ThreadState, new: ThreadState, success: Ordering, failure: Ordering) -> Result<ThreadState, ThreadState> {
		let current = into_raw(current);
		let new = into_raw(new);
		self.0.compare_exchange(current, new, success, failure)
				.map(from_raw)
				.map_err(from_raw)
	}

	pub fn fetch_update(&self, set_order: Ordering, fetch_order: Ordering, mut f: impl FnMut(ThreadState) -> Option<ThreadState>) -> Result<ThreadState, ThreadState> {
		self.0.fetch_update(set_order, fetch_order, |val| f(from_raw(val)).map(into_raw))
				.map(from_raw)
				.map_err(from_raw)
	}
	
	pub fn running(&self) -> bool {
		matches!(
			self.load(Ordering::SeqCst),
			ThreadState::Running,
		)
	}

	pub fn runnable(&self) -> bool {
		matches!(
			self.load(Ordering::SeqCst),
			ThreadState::Running | ThreadState::Ready | ThreadState::NearlyParked,
		)
	}
}

impl Debug for AtomicThreadState {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "AtomicThreadState {{ {:?} }}", self.load(Ordering::Relaxed))
	}
}

#[derive(Debug)]
#[repr(u8)]
pub enum ThreadState {
	/// The thread is able to run, but has not yet been scheduled
	Ready = 0,
	/// The thread is actively running
	Running = 1,
	/// The thread is parked
	Parked = 2,
	/// The thread is in the process of being parked
	NearlyParked = 3,
	Killed(i8) = 4,
}
