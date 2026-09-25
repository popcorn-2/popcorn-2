//! Provides an interface to the scheduler and threading subsystem

mod state;

use core::borrow::Borrow;
use core::ptr::NonNull;
pub use state::*;
use crate::ptr::TaggedNonNull;

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

	#[doc(hidden)]
	pub unsafe fn from_raw(ptr: TaskRefInner) -> Self {
		Self {
			tagged_ref: ptr,
		}
	}

	#[doc(hidden)]
	pub fn tag_raw(self) -> usize {
		self.tagged_ref.tag()
	}

	pub fn as_ptr(self) -> NonNull<u64> { self.tagged_ref.as_ptr() }
	pub fn generation(self) -> usize { self.tagged_ref.tag() >> TaskRefInner::TAG_LOW_BITS }
	pub fn addr_eq(self, other: TaskRef) -> bool {
		TaskRefInner::addr_eq(&self.tagged_ref, &other.tagged_ref)
	}
}
