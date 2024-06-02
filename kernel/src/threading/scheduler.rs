#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, VecDeque};
use core::borrow::Borrow;
use core::cell::{Cell, UnsafeCell};
use core::cmp::min;
use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::num::NonZero;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use crate::hal;
use crate::threading::tcb::{ThreadControlBlock, ThreadState};
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "preemptive")] use core::time::Duration;
use kernel_api::memory::physical::highmem;
use kernel_api::sync::{IrqCell, IrqGuard};
use kernel_api::time::Instant;
use crate::hal::paging2::TTable;
use crate::hal::timing::{Timer, Eoi};
use crate::interrupts::irq_handler;
use crate::memory::paging::ktable;
use crate::threading::{EventTy, SchedulerEvent};

#[thread_local]
pub static SCHEDULER: IrqCell<Scheduler> = IrqCell::new(Scheduler::new());

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Tid(pub(super) usize);

impl Tid {
	fn new() -> Self {
		static TIDS: AtomicUsize = AtomicUsize::new(1);
		Self(TIDS.fetch_add(1, Ordering::Relaxed))
	}
}

#[derive(Debug)]
pub struct Scheduler {
	tasks: BTreeMap<Tid, ThreadControlBlock>,
	run_queue: VecDeque<Tid>,
	current_tid: Tid,
	pub event_queue: EventQueue,
	cleanup_queue: VecDeque<Tid>,
}

#[derive(Debug)]
pub struct EventQueue {
	events: VecDeque<SchedulerEvent>,
	#[cfg(feature = "preemptive")] yield_event: Option<(Instant, Tid)>,
}

impl EventQueue {
	pub fn add(&mut self, event: SchedulerEvent) {
		self.events.push_back(event);
		self.events.make_contiguous().sort_unstable();
	}
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct DuplicateKey;

trait BTreeExt<K, V> {
	fn get_many_mut<const N: usize, Q>(&mut self, keys: [&Q; N]) -> Result<[Option<&mut V>; N], DuplicateKey> where K: Borrow<Q> + Ord, Q: Ord + ?Sized;
}

impl<K, V, A: core::alloc::Allocator + Clone> BTreeExt<K, V> for BTreeMap<K, V, A> {
	fn get_many_mut<const N: usize, Q>(&mut self, keys: [&Q; N]) -> Result<[Option<&mut V>; N], DuplicateKey> where K: Borrow<Q> + Ord, Q: Ord + ?Sized {
		fn get_ptr<Q, K, V, A: core::alloc::Allocator + Clone>(this: &mut BTreeMap<K, V, A>, key: &Q) -> Option<NonNull<V>> where K: Borrow<Q> + Ord, Q: Ord + ?Sized {
			this.get_mut(key).map(NonNull::from)
		}

		let mut ptrs = [MaybeUninit::<Option<NonNull<V>>>::uninit(); N];

		for (i, &cur) in keys.iter().enumerate() {
			let ptr = get_ptr(self, cur);

			if ptrs[..i].iter().any(|&prev| unsafe { *prev.assume_init_ref() } == ptr) {
				return Err(DuplicateKey);
			}

			ptrs[i].write(ptr);
		}

		Ok(unsafe { ptrs.transpose().assume_init() }.map(|ptr| ptr.map(|mut ptr| unsafe { ptr.as_mut() })))
	}
}

extern "C" fn thread_cleaner(_: ()) -> ! {
	loop {
		let mut guard = SCHEDULER.lock();
		let scheduler = &mut *guard;
		
		for tid in scheduler.cleanup_queue.drain(..) {
			if scheduler.tasks.remove(&tid).is_none() {
				warn!("Deletion of {tid:?} failed");
			}
		}
		drop(guard);
		super::block(ThreadState::Blocked);
	}
}

impl Scheduler {
	pub const fn new() -> Self {
		Self {
			tasks: BTreeMap::new(),
			run_queue: VecDeque::new(),
			current_tid: Tid(0),
			event_queue: EventQueue {
				events: VecDeque::new(),
				#[cfg(feature = "preemptive")] yield_event: None,
			},
			cleanup_queue: VecDeque::new(),
		}
	}
	
	pub fn queue_for_deletion(&mut self, exit_code: i8) {
		let tid = self.current_tid();
		#[cfg(feature = "log.scheduler")] debug!("delete {tid:?}");
		if exit_code != 0 && let Some(tcb) = self.tasks.get(&tid) {
			error!("Task `{}` exited with status code {exit_code}", tcb.name);
		}
		self.cleanup_queue.push_back(tid);
		self.unblock(Tid(1));
		self.block(ThreadState::AwaitingDeletion);
	}

	fn wake_and_reset_timer(&mut self) {
		#[cfg(feature = "log.scheduler")] debug!("event queue: {:#?}", self.event_queue);

		let mut local_timer = hal::LocalTimer::get();
		let tick_period = local_timer.get_time_period_picos().unwrap() * 4;

		let now = Instant::now();

		let ticks_to_event = |time: Instant| {
			let time = time.saturating_duration_since(now);
			let ticks = 1000 *  time.as_nanos() / u128::from(tick_period);
			NonZero::<u128>::new(ticks)
		};

		let mut timer_ticks = None::<NonZero<u128>>;

		loop {
			let Some(event) = self.event_queue.events.get(0) else { break; };
			match ticks_to_event(event.time) {
				None => {
					#[cfg(feature = "log.scheduler")] debug!("event {event:?} in past - handling now");
					let event = self.event_queue.events.pop_front().expect("Already peeked at this event");
					self.handle_event(event);
				},
				Some(event_ticks) => {
					timer_ticks = Some(timer_ticks.map(|current| min(current, event_ticks)).unwrap_or(event_ticks));
					break;
				},
			}
		}

		#[cfg(feature = "preemptive")]
		if let Some(yield_event) = self.event_queue.yield_event {
			match ticks_to_event(yield_event.0) {
				None => {
					#[cfg(feature = "log.scheduler")]  debug!("{:?} yield in past - handling now", yield_event.1);
					self.event_queue.yield_event.take().expect("Already peeked at this event");
					super::defer_schedule();
				},
				Some(event_ticks) => {
					timer_ticks = Some(timer_ticks.map(|current| min(current, event_ticks)).unwrap_or(event_ticks));
				},
			}
		}

		if let Some(ticks) = timer_ticks {
			#[cfg(feature = "log.scheduler")] debug!("setting oneshot timer for {ticks} ticks");
			local_timer.set_oneshot_time(ticks.get()).unwrap();
		}
	}

	pub fn init(&mut self, tid0: ThreadControlBlock) {
		assert!(self.tasks.insert(Tid(0), tid0).is_none(), "Cannot init scheduler multiple times");
		
		let ttable = hal::TTableTy::new(&*ktable(), highmem()).unwrap();
		self.add_task(ThreadControlBlock::new(
			Cow::Borrowed("Thread cleanup"),
			ttable,
			super::thread_startup,
			thread_cleaner,
			(),
		));

		let mut local_timer = hal::LocalTimer::get();
		local_timer.set_irq_number(0x40).unwrap();
		local_timer.set_divisor(4).unwrap();

		let eoi_handle = local_timer.eoi_handle();

		let timer_irq = irq_handler!(move || {
			main => {
				let mut guard = SCHEDULER.lock();
				guard.wake_and_reset_timer();
			}
			eoi => {
				eoi_handle.send();
			}
		});

		let scheduler_defer_irq = move || {
			let mut guard = SCHEDULER.lock();
			// FIXME: only true with LAPIC
			eoi_handle.send(); // safe to send this now since interrupts are disabled by scheduler lock
			guard.schedule();
			IrqGuard::unlock_no_interrupts(guard);
		};

		assert!(crate::interrupts::insert_handler(0x40, timer_irq).is_ok());
		crate::interrupts::set_defer_irq(scheduler_defer_irq);
	}

	pub fn current_tid(&self) -> Tid { self.current_tid }

	pub fn add_task(&mut self, tcb: ThreadControlBlock) -> Tid {
		let tid = Tid::new();
		self.tasks.insert(tid, tcb);
		self.run_queue.push_back(tid);
		super::defer_schedule();
		tid
	}

	pub fn schedule(&mut self) {
		#[cfg(feature = "preemptive")]
		let quantum = |_tid| {
			Duration::from_millis(50) // TODO: make this dynamic based on priority?
		};

		#[cfg(feature = "log.scheduler")] debug!("task schedule");
		if let Some(new_tid) = self.run_queue.pop_front() {
			let old_tid = self.current_tid;
			self.current_tid = new_tid;

			#[cfg(feature = "preemptive")] {
				self.event_queue.yield_event = Some((
					Instant::now() + quantum(new_tid),
					new_tid
				));
			}
			self.wake_and_reset_timer();

			let [old_tcb, new_tcb] = self.tasks.get_many_mut([&old_tid, &new_tid]).expect("Can't switch to same task");
			let old_tcb = old_tcb.expect("Cannot have been running a task that doesn't exist");
			let new_tcb = new_tcb.expect("Next task in queue has already exited");

			if old_tcb.state == ThreadState::Running {
				old_tcb.state = ThreadState::Ready;
				self.run_queue.push_back(old_tid);
			}

			new_tcb.state = ThreadState::Running;
			#[cfg(feature = "log.scheduler")] debug!("[scheduler] switch {old_tid:?} -> {new_tid:?}");

			unsafe {
				hal::switch_thread(old_tcb, new_tcb);
			}
		} else {
			#[cfg(feature = "log.scheduler")] debug!("no other tasks to run");

			let current_tid = self.current_tid;
			let current_tcb = self.tasks.get(&current_tid).expect("Cannot have been running a task that doesn't exist");

			if current_tcb.state == ThreadState::Running {
				// No other tasks can get added to the run queue without an interrupt occuring so don't need to manually preempt
				#[cfg(feature = "preemptive")] self.event_queue.yield_event.take();
				self.wake_and_reset_timer();

				return;
			}

			todo!("idling");
		}
	}

	pub fn block(&mut self, state: ThreadState) {
		let current_tcb = self.tasks.get_mut(&self.current_tid).expect("Cannot have been running a task that doesn't exist");
		current_tcb.state = state;

		#[cfg(feature = "log.scheduler")] debug!("blocking {:?}", self.current_tid);

		// Remove the yield event for this task to not spuriously cut short a different task
		#[cfg(feature = "preemptive")] self.event_queue.yield_event.take();
		#[cfg(feature = "log.scheduler")] debug!("timer reset - block");
		self.wake_and_reset_timer();

		super::defer_schedule();
	}

	pub fn unblock(&mut self, tid: Tid) {
		#[cfg(feature = "log.scheduler")] debug!("unblocking {:?}", tid);
		if let Some(tcb) = self.tasks.get_mut(&tid) {
			if tcb.state != ThreadState::Ready && tcb.state != ThreadState::Running {
				tcb.state = ThreadState::Ready;
				self.run_queue.push_back(tid);
				super::defer_schedule();
			}
		} else { warn!("Attempted to unblock dead {tid:?}"); }
	}

	fn handle_event(&mut self, event: SchedulerEvent) {
		#[cfg(feature = "log.scheduler")] debug!("scheduler event: {event:?}");

		match event.action {
			EventTy::Unblock => self.unblock(event.tid),
		}
	}
}

#[cfg(test)]
mod tests {
	use alloc::collections::BTreeMap;
	use super::*;

	#[test]
	fn btree_dup_key() {
		let mut tree = BTreeMap::from([(1, true), (2, false), (3, true)]);
		assert_eq!(tree.get_many_mut([&1, &1]), Err(DuplicateKey));
	}
}
