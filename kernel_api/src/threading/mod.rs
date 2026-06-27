//! Provides an interface to the scheduler and threading subsystem.

mod meta;
mod state;

use core::borrow::Borrow;
pub use meta::*;
pub use state::*;

/// The numerical ID of a thread.
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
	/// Create a new `ThreadId`.
	#[must_use]
	pub const fn new(id: isize) -> Self { Self { id } }

	/// Get the internal value of the `ThreadId`.
	#[must_use]
	pub const fn get(self) -> isize { self.id }
}

impl Borrow<ThreadId> for ThreadMeta {
	fn borrow(&self) -> &ThreadId {
		&self.thread_id
	}
}
