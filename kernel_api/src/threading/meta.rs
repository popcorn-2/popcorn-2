use alloc::sync::Arc;
use core::sync::atomic::Ordering;
use core::task::Poll;
use futures::task::AtomicWaker;
use log::warn;
use crate::address_space::{AddressSpace, Kernel};
use crate::mapping::{Mapping, Stack};
use crate::syscall::AsyncMap;
use crate::syscall::handle::HandleMap;
use crate::threading::{AtomicThreadState, ThreadId, ThreadState};

// todo: decide what we want `pub` here
#[derive(Debug)]
pub struct ThreadMeta {
	pub name: Arc<str>,
	pub kernel_stack: Mapping<Stack, Kernel>,
	pub state: AtomicThreadState,
	pub thread_id: ThreadId,
	pub address_space: AddressSpace,
	pub handles: HandleMap,
	pub async_map: Arc<AsyncMap>,
	pub join_waiter: AtomicWaker,
}

impl ThreadMeta {
	pub async fn join(self: Arc<Self>) -> isize {
		core::future::poll_fn(|ctx| {
			self.join_waiter.register(ctx.waker());
			match self.state.load(Ordering::SeqCst) {
				ThreadState::Killed(code) => Poll::Ready(code),
				_ => Poll::Pending,
			}
		}).await
	}
}

impl Drop for ThreadMeta {
	fn drop(&mut self) {
		warn!("todo: clean up memory for thread `{}`", self.name);
	}
}
