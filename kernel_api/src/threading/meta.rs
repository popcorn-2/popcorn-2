use alloc::sync::Arc;
use crate::address_space::{AddressSpace, Kernel};
use crate::mapping::{Mapping, Stack};
use crate::syscall::AsyncMap;
use crate::syscall::handle::HandleMap;
use crate::threading::{AtomicThreadState, ThreadId};

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
}

impl ThreadMeta {
	pub async fn join(self: Arc<Self>) -> i8 { todo!() }
}
