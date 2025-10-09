use alloc::collections::{BTreeSet, VecDeque};
use alloc::sync::{Arc, Weak};
use core::fmt::Debug;
use crate::threading::scheduler::Scheduler;
use crate::threading::ThreadControlBlock;
use kernel_api::sync::Spinlock;
use kernel_api::threading::ThreadId;

mod event;

#[derive(Debug)]
pub struct TicklessRoundRobin {
	run_queue: Arc<Spinlock<VecDeque<ThreadControlBlock>>>,
	blocked_list: BTreeSet<ThreadControlBlock>,
	//event_queue: Queue,
}

#[derive(Debug)]
pub struct Injector {
	queue: Weak<Spinlock<VecDeque<ThreadControlBlock>>>
}

impl super::Injector for Injector {
	fn enqueue(&self, thread: ThreadControlBlock) -> Result<(), ThreadControlBlock> {
		let queue = match self.queue.upgrade() {
			Some(queue) => queue,
			None => {
				warn!("Attempted to inject into dead task queue");
				return Err(thread);
			},
		};
		
		queue.lock()
				.push_back(thread);
		Ok(())
	}
}

#[derive(Debug)]
pub struct Stealer {}
impl super::Stealer for Stealer {}

impl Scheduler for TicklessRoundRobin {
	fn new() -> (Self, Box<dyn super::Injector>, Arc<dyn super::Stealer>) where Self: Sized {
		let this = Self {
			run_queue: Arc::new(Spinlock::new(VecDeque::new())),
			blocked_list: BTreeSet::new(),
			//event_queue: Queue::new(),
		};
		let queue = Arc::downgrade(&this.run_queue);
		
		(
			this,
			Box::new(Injector { queue }),
			Arc::new(Stealer {})
		)
	}

	fn get_next_thread_(&mut self) -> Option<ThreadControlBlock> {
		self.run_queue.lock().pop_front()
	}

	fn put_thread(&mut self, old_thread: ThreadControlBlock) {
		let state = &old_thread.state;
		debug_assert!(!state.running());
		
		if state.runnable() {
			self.enqueue(old_thread);
		} else {
			self.blocked_list.insert(old_thread);
		}
	}

	fn on_thread_exit(&mut self, tid: ThreadId) {
		let _ = self.blocked_list.remove(&tid);
	}

	fn enqueue(&mut self, thread: ThreadControlBlock) {
		self.run_queue.lock().push_back(thread);
	}

	fn unpark(&mut self, thread_id: ThreadId) {
		if let Some(tcb) = self.blocked_list.take(&thread_id) {
			self.run_queue.lock().push_back(tcb);
		} else {
			warn!("attempted to unpark non-existent thread {:?}", thread_id);
		}
	}

	/*
	fn unpark(&mut self, thread_id: ThreadId, reason: WakeReason) -> Result<(), ()> {
		debug!("scheduler local unpark of {thread_id:?} for {reason:?}");
		let guard = percpu_v2!(current_thread).read();
		let t = guard.as_ref().ok_or(())?;
		if *t.tcb_ref().thread_id == thread_id {
			t.tcb_ref().state.store(ThreadState::JustUnparked(reason), Ordering::SeqCst);
		} else { debug!("non-active thread so must be running already"); }
		Ok(())
	}

	fn kill(&mut self, thread_id: ThreadId) -> Result<(), ()> {
		let mut guard = self.run_queue.lock();
		let idx = guard.iter().position(|thread| *thread.tcb_ref().thread_id == thread_id)
				.ok_or(())?;
		let mut thread = guard.remove(idx).expect("just found this from iterator position");
		thread.tcb_mut().state.store(ThreadState::Dead, Ordering::SeqCst);
		threading::move_to_global_parking_lot(thread);
		Ok(())
	}
	*/
}
