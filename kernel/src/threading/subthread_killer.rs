use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use log::error;
use kernel_api::sync::Spinlock;
use crate::threading::ThreadId;

#[derive(Debug, Clone)]
pub struct SubthreadKiller {
	inner: Arc<SubthreadKillerInner>
}

#[derive(Debug)]
struct SubthreadKillerInner {
	dead: AtomicBool,
	list: Spinlock<Vec<ThreadId>>,
}

impl SubthreadKiller {
	pub fn new(thread_id: ThreadId) -> Self {
		Self {
			inner: Arc::new(SubthreadKillerInner {
				dead: AtomicBool::new(false),
				list: Spinlock::new(vec![thread_id]),
			})
		}
	}
	
	pub fn add_thread(&self, thread_id: ThreadId) {
		self.inner.list.lock().push(thread_id);
	}
}

impl SubthreadKillerInner {
	fn kill(&self) {
		if self.dead.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
			for thread in &*self.list.lock() {
				super::kill(*thread);
			}
		}
	}
}

impl Drop for SubthreadKiller {
	fn drop(&mut self) {
		self.inner.kill();
	}
}

impl Drop for SubthreadKillerInner {
	fn drop(&mut self) {
		if !self.dead.load(Ordering::SeqCst) {
			error!("subthread killer dropped without killing");
			self.kill();
		}
	}
}
