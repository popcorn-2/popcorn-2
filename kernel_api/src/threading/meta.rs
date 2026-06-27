use alloc::sync::Arc;
use core::sync::atomic::Ordering;
use core::task::Poll;
use futures::task::AtomicWaker;
use log::warn;
use crate::address_space;
use crate::mapping::{Mapping, Stack};
use crate::syscall::AsyncMap;
use crate::syscall::handle::HandleMap;
use crate::threading::{AtomicThreadState, ThreadId, ThreadState};

/// Owned state of a thread.
///
/// This holds ownership over resources required to run a thread, such as
/// the memory mapping for the kernel stack.
// todo: decide what we want `pub` here
#[derive(Debug)]
pub struct ThreadMeta {
	#[doc(hidden)] pub name: Arc<str>,
	#[doc(hidden)] pub kernel_stack: Mapping<Stack, address_space::Kernel>,
	#[doc(hidden)] pub state: AtomicThreadState,
	#[doc(hidden)] pub thread_id: ThreadId,
	#[doc(hidden)] pub address_space: address_space::User,
	#[doc(hidden)] pub handles: HandleMap,
	#[doc(hidden)] pub async_map: Arc<AsyncMap>,
	#[doc(hidden)] pub join_waiter: AtomicWaker,
}

impl ThreadMeta {
	/// Returns the exit code of a thread when it exists.
	pub fn join(self: Arc<Self>) -> impl Future<Output = isize> {
		core::future::poll_fn(move |ctx| {
			self.join_waiter.register(ctx.waker());
			match self.state.load(Ordering::SeqCst) {
				ThreadState::Killed(code) => Poll::Ready(code),
				_ => Poll::Pending,
			}
		})
	}
}

impl Drop for ThreadMeta {
	fn drop(&mut self) {
		warn!("todo: clean up memory for thread `{}`", self.name);
	}
}
