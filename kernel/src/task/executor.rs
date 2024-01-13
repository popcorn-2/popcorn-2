use alloc::boxed::Box;
use alloc::collections::VecDeque;
use core::future::Future;
use core::task::{Context, Poll, Waker};
use crate::task::Task;

pub struct Executor {
	tasks: VecDeque<Task>
}

impl Executor {
	pub fn new() -> Self {
		Self {
			tasks: VecDeque::new()
		}
	}

	pub fn spawn<F: Future<Output = ()> + 'static>(&mut self, task: impl FnOnce() -> F) {
		let task = Task {
			future: Box::pin(task())
		};
		self.tasks.push_back(task);
	}

	pub fn run(&mut self) {
		while let Some(mut task) = self.tasks.pop_front() {
			let waker = Waker::noop();
			let mut context = Context::from_waker(&waker);
			match task.poll(&mut context) {
				Poll::Ready(_) => {}
				Poll::Pending => self.tasks.push_back(task)
			}
		}
	}
}
