use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::task::Wake;
use core::future::Future;
use core::task::{Context, Poll};
use crossbeam_queue::ArrayQueue;
use crate::task::Task;

pub struct Executor {
	next_task_id: usize,
	pending_run: Arc<ArrayQueue<usize>>,
	tasks: BTreeMap<usize, Task>,
	wakers: BTreeMap<usize, core::task::Waker>
}

impl Executor {
	pub fn new() -> Self {
		Self {
			next_task_id: 0,
			pending_run: Arc::new(ArrayQueue::new(128)),
			tasks: BTreeMap::new(),
			wakers: BTreeMap::new()
		}
	}

	pub fn spawn<F: Future<Output = ()> + 'static>(&mut self, task: impl FnOnce() -> F) {
		let id = self.next_task_id;
		self.next_task_id += 1;
		let task = Task {
			future: Box::pin(task()),
		};
		self.pending_run.push(id)
		    .expect("Unable to spawn task");
		self.tasks.insert(id, task);

		let waker = Arc::new(Waker {
			list: self.pending_run.clone(),
			task_to_wake: id
		});
		self.wakers.insert(id, waker.into());
	}

	pub fn run(&mut self) -> ! {
		loop {
			while let Some(id) = self.pending_run.pop() {
				let task = self.tasks.get_mut(&id)
				               .expect("Task does not exist");

				let mut context = Context::from_waker(self.wakers.get(&id).unwrap());
				match task.poll(&mut context) {
					Poll::Ready(_) => {
						self.tasks.remove(&id);
						self.wakers.remove(&id);
					}
					Poll::Pending => {}
				}
			}
		}
	}
}

struct Waker {
	list: Arc<ArrayQueue<usize>>,
	task_to_wake: usize
}

impl Wake for Waker {
	fn wake(self: Arc<Self>) {
		self.wake_by_ref();
	}

	fn wake_by_ref(self: &Arc<Self>) {
		self.list.push(self.task_to_wake)
		    .expect("Unable to wake task")
	}
}
