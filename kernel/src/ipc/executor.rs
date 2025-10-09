use alloc::sync::{Arc, Weak};
use core::pin::Pin;
use core::sync::atomic::Ordering;
use core::task::{Context, Poll, Waker};
use kernel_api::channel;
use kernel_api::channel::Receiver;
use kernel_api::executor::block_on;
use kernel_api::sync::LazyLock;
use kernel_api::threading::ThreadMeta;
use crate::threading;

struct Task {
	owner: Weak<ThreadMeta>,
	future: Pin<Box<dyn Future<Output = ()>>>,
}

const _: () = {
	const fn check_send_sync<T: Send + Sync + ?Sized>() {}

	check_send_sync::<Weak<ThreadMeta>>();

	unsafe impl Send for Task {}
};


struct Executor {
	channel: Receiver<Task>,
	#[expect(unused)]
	thread: Arc<ThreadMeta>,
}

impl Executor {
	fn push_task(&self, future: Pin<Box<dyn Future<Output = ()> + 'static>>) {
		let owner = Arc::downgrade(
			percpu_v2!(current_thread)
					.read()
					.as_ref()
					.expect("cannot push task from idle")
					.meta()
		);

		self.channel.push(Task {
			owner,
			future,
		})
	}

	fn main(&self) -> ! {
		loop {
			let mut incomplete = Vec::with_capacity(self.channel.len());

			let mut ctx = Context::from_waker(Waker::noop());
			let mut handle_task = |mut task: Task| {
				if let Some(owner) = Weak::upgrade(&task.owner) {
					let guard = percpu_v2!(current_thread).read();
					let thread = guard.as_ref().expect("executor must run on thread");

					let old = unsafe {
						thread
							.address_space
							.swap(owner.address_space.clone(), Ordering::SeqCst)
					};
					unsafe { owner.address_space.load(); }
					unsafe { thread.handles.swap(owner.handles.clone(), Ordering::SeqCst) };
					drop(guard);

					match task.future.as_mut().poll(&mut ctx) {
						Poll::Ready(_) => trace!("finished async syscall for `{}`({:?})", owner.name, owner.thread_id),
						Poll::Pending => {
							trace!("async syscall for `{}`({:?}) pending", owner.name, owner.thread_id);
							incomplete.push(task);
						},
					}

					unsafe {
						old.load();
						percpu_v2!(current_thread)
								.read()
								.as_ref()
								.expect("executor must run on thread")
								.address_space
								.swap(old, Ordering::SeqCst);
					}
				}
			};

			handle_task(block_on(self.channel.pop()));
			while let Some(task) = self.channel.try_pop() {
				handle_task(task);
			}

			for task in incomplete {
				self.channel.push(task);
			}
			threading::yield_now(); // fixme: replace with using actual waker for child tasks
		}
	}
}

#[unsafe(export_name = "__popcorn_async_spawn_task")]
fn spawn(task: Pin<Box<dyn Future<Output = ()> + 'static>>) {
	static EXECUTOR: LazyLock<Executor> = LazyLock::new(|| {
		let thread = threading::spawn_kernel("kpool0".into(), || {
			EXECUTOR.main()
		}).unwrap();

		debug!("spawned executor as {thread:?}");

		Executor {
			channel: channel::unbounded().1,
			thread,
		}
	});

	EXECUTOR.push_task(task);
}
