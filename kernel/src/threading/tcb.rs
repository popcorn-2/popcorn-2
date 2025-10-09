use alloc::sync::Arc;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::ops::Deref;
use kernel_api::allocator::AllocError;
use kernel_api::threading::{ThreadId, ThreadMeta};
use crate::hal;
use crate::hal::SaveStateTr;

#[derive(Debug)]
pub struct ThreadControlBlock {
	pub register_state: hal::SaveState,
	pub meta: Arc<ThreadMeta>,
}

impl ThreadControlBlock {
	pub fn new(mut meta: ThreadMeta, main: extern "C" fn(usize) -> !, arg: usize) -> Result<(Self, Arc<ThreadMeta>), AllocError> {
		let register_state = hal::SaveState::new(
			&mut meta.kernel_stack,
			main,
			arg,
		)?;

		let tcb = ThreadControlBlock {
			register_state,
			meta: Arc::new(meta),
		};
		let meta = Arc::clone(&tcb.meta);
		Ok((tcb, meta))
	}

	pub fn meta(&self) -> &Arc<ThreadMeta> {
		&self.meta
	}
}

impl Deref for ThreadControlBlock {
	type Target = ThreadMeta;

	fn deref(&self) -> &Self::Target {
		&self.meta
	}
}

impl Borrow<ThreadId> for ThreadControlBlock {
	fn borrow(&self) -> &ThreadId {
		&self.thread_id
	}
}

impl PartialEq for ThreadControlBlock {
	fn eq(&self, other: &Self) -> bool {
		self.thread_id == other.thread_id
	}
}

impl Eq for ThreadControlBlock {}

impl PartialOrd for ThreadControlBlock {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for ThreadControlBlock {
	fn cmp(&self, other: &Self) -> Ordering {
		self.thread_id.cmp(&other.thread_id)
	}
}
