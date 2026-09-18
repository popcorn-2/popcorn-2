//! Provides an interface to the scheduler and threading subsystem

mod meta;
mod state;

use core::borrow::Borrow;
use core::ptr::NonNull;
pub use meta::*;
pub use state::*;
use crate::ptr::TaggedNonNull;

/// The numerical ID of a thread
/// 
/// This is the same as the internal handle number by the `proc` server for
/// this thread.
///
/// **This is only unique for non-kernel threads while they are alive**.
/// All kernel threads have an invalid `ThreadId`, and `ThreadId`s will be reused
/// after a thread is killed.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ThreadId {
	id: isize,
}

impl ThreadId {
	pub const fn new(id: isize) -> Self { Self { id } }

	pub const fn get(self) -> isize { self.id }
}

impl Borrow<ThreadId> for ThreadMeta {
	fn borrow(&self) -> &ThreadId {
		&self.thread_id
	}
}

type TaskRefInner = TaggedNonNull<u64>;

/// A cheap to copy handle to a task.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, PartialEq, Eq, Hash)]
pub struct TaskRef {
	tagged_ref: TaskRefInner,
}

// SAFETY: constructor only allows pointers to `Sync` types so `Send` references are sound
unsafe impl Send for TaskRef {}
// SAFETY: constructor only allows pointers to `Sync` types so `Sync` references are sound
unsafe impl Sync for TaskRef {}

impl TaskRef {
	pub const MAX_GENERATION: usize = (1 << TaskRefInner::TAG_HIGH_BITS) - 1;

	#[doc(hidden)]
	pub unsafe fn new<T: Sync>(r: &'static T, generation: usize) -> Self {
		Self {
			tagged_ref: TaskRefInner::new(
				NonNull::from_ref(r).cast(),
				generation << TaskRefInner::TAG_LOW_BITS,
			),
		}
	}

	pub fn as_ptr(self) -> NonNull<u64> { self.tagged_ref.as_ptr() }
	pub fn generation(self) -> usize { self.tagged_ref.tag() >> TaskRefInner::TAG_LOW_BITS }
	pub fn addr_eq(self, other: TaskRef) -> bool {
		TaskRefInner::addr_eq(&self.tagged_ref, &other.tagged_ref)
	}
}
