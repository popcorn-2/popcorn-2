//! Provides functions for interacting with the kernel's async executor.

use alloc::sync::Arc;
use alloc::task::Wake;
use core::pin::pin;
use core::sync::atomic::Ordering;
use core::task::{Context, Poll, Waker};
use log::{debug, trace};
use crate::threading::{ThreadMeta, ThreadState};

impl Wake for ThreadMeta {
	fn wake(self: Arc<Self>) {
		self.wake_by_ref();
	}

	fn wake_by_ref(self: &Arc<Self>) {
		trace!("wake thread {:?}", self.thread_id);
		self.state.store(ThreadState::Ready, Ordering::SeqCst);
		crate::bridge::threading::unblock_thread(self);
	}
}

/// Blocks the current thread until the future is ready, parking the thread where possible.
#[cfg(not(feature = "use_std"))]
pub fn block_on<T, F: Future<Output = T>>(f: F) -> T {
	let mut f = pin!(f);

	loop {
		let waker = {
			let meta = crate::bridge::threading::with_current_thread(|meta| {
				meta.state.store(ThreadState::NearlyParked, Ordering::SeqCst);
				Arc::clone(meta)
			});
			Waker::from(meta)
		};

		let mut ctx = Context::from_waker(&waker);

		match f.as_mut().poll(&mut ctx) {
			Poll::Ready(val) => {
				debug!("future ready!");
				crate::bridge::threading::with_current_thread(|meta,| {
					let _ = meta.state
					    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |state| {
						    debug!("block_on replace state {state:?}");
						    match state {
							    ThreadState::Ready | ThreadState::Parked | ThreadState::NearlyParked => Some(ThreadState::Running),
							    _ => None,
						    }
					    });
				});

				break val;
			},
			Poll::Pending => {
				debug!("future pending :(");
				crate::bridge::executor::yield_from_async_block();
			}
		}
	}
}

/// Spawns a future on the kernel's async executor.
///
/// The future will run to completion in the background in the context of the current thread.
/// This means that any access via (`LocalUser`)[`crate::ptr::LocalUser`] will be to the current thread's
/// address space.
/// This also means that there is no requirement for the future to be [`Send`] nor [`Sync`].
pub fn spawn(f: impl Future<Output = ()> + 'static) {
	crate::bridge::executor::spawn_task(Box::pin(f));
}
