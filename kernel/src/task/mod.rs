use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub mod executor;

struct Task {
	future: Pin<Box<dyn Future<Output=()>>>
}

impl Task {
	fn poll(&mut self, context: &mut Context) -> Poll<()> {
		self.future.as_mut().poll(context)
	}
}
