use alloc::borrow::Cow;
use core::arch::asm;
use core::cmp::Ordering;
use core::num::NonZeroUsize;
use core::time::Duration;
use log::{debug, warn};
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::{highmem, OwnedFrames};
use kernel_api::memory::r#virtual::{Global, OwnedPages};
use kernel_api::time::Instant;
use crate::hal::{Hal, HalTy, ThreadControlBlock, ThreadState};
use scheduler::Tid;
use crate::hal::timing::{Timer, Eoi};
use crate::interrupts::irq_handler;

pub mod scheduler;

pub unsafe fn init(handoff_data: crate::HandoffWrapper) -> Tid {
	let stack = handoff_data.memory.stack;
	let ttable = handoff_data.to_empty_ttable();

	// fixme: is highmem always correct?
	let stack_phys_len = stack.top_virt - stack.bottom_virt - 1;
	let stack_frames = OwnedFrames::from_raw_parts(
		stack.top_phys - stack_phys_len,
		NonZeroUsize::new(stack_phys_len).expect("Cannot have a zero sized stack"),
		highmem()
	);
	let stack_pages = OwnedPages::from_raw_parts(
		stack.bottom_virt,
		NonZeroUsize::new(stack_phys_len + 1).expect("Cannot have a zero sized stack"),
		Global
	);

	let mut scheduler = scheduler::SCHEDULER.lock();
	let tcb =  ThreadControlBlock {
		name: Cow::Borrowed("init"),
		kernel_stack: Stack::from_raw_parts(stack_frames, stack_pages),
		ttable,
		state: ThreadState::Running,
		save_state: Default::default(),
	};
	assert!(scheduler.tasks.insert(Tid(0), tcb).is_none());

	let mut local_timer = <HalTy as Hal>::LocalTimer::get();
	local_timer.set_irq_number(0x40).unwrap();
	local_timer.set_divisor(4).unwrap();

	let eoi_handle = local_timer.eoi_handle();

	let timer_irq = irq_handler!(move || {
		main => {
			let mut guard = scheduler::SCHEDULER.lock();
			if let Some(to_wake) = guard.sleep_queue.pop_front() {
				debug!("supposed to wake Tid {:?}", to_wake.tid);
				guard.unblock(to_wake.tid);
			} else {
				warn!("Spurious scheduler timer irq");
			}
			recalculate_timer(&mut guard);
		}
		eoi => {
			eoi_handle.send();
		}
	});
	
	let scheduler_defer_irq = move || {
		let mut guard = scheduler::SCHEDULER.lock();
		// FIXME: only true with LAPIC
		eoi_handle.send(); // safe to send this now since interrupts are disabled by scheduler lock
		guard.schedule();
	};

	assert!(crate::interrupts::insert_handler(0x40, timer_irq).is_none());
	crate::interrupts::set_defer_irq(scheduler_defer_irq);

	Tid(0)
}

pub fn thread_yield() {
	defer_schedule();
}

pub fn block(reason: ThreadState) {
	scheduler::SCHEDULER.lock().block(reason);
}

#[derive(Debug)]
struct SleepingTid {
	time_of_wake: Instant,
	tid: Tid,
}

impl Ord for SleepingTid {
	fn cmp(&self, other: &Self) -> Ordering {
		self.time_of_wake.cmp(&other.time_of_wake)
	}
}

impl PartialOrd for SleepingTid {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl PartialEq for SleepingTid {
	fn eq(&self, other: &Self) -> bool {
		self.time_of_wake.eq(&other.time_of_wake)
	}
}

impl Eq for SleepingTid {}

fn push_to_global_sleep_queue(wake_time: Instant) {
	todo!()
}

fn recalculate_timer(scheduler: &mut scheduler::Scheduler) {
	let mut local_timer = <HalTy as Hal>::LocalTimer::get();
	let tick_period = local_timer.get_time_period_picos().unwrap() * 4;
	
	loop {
		let Some(next) = scheduler.sleep_queue.get(0) else { break; };
		debug!("{:?}", next.time_of_wake);
		
		let time_to_wake = next.time_of_wake.saturating_duration_since(Instant::now());
		debug!("wake {:?} in {time_to_wake:?}", next.tid);
		
		if time_to_wake == Duration::from_secs(0) {
			let next = scheduler.sleep_queue.pop_front().expect("Already peeked so should not have gone");
			scheduler.unblock(next.tid);
		} else {
			let ticks = time_to_wake.as_nanos().saturating_div(u128::from(tick_period) / 1000);
			if ticks == 0 {
				let next = scheduler.sleep_queue.pop_front().expect("Already peeked so should not have gone");
				scheduler.unblock(next.tid);
			} else {
				local_timer.set_oneshot_time(ticks).unwrap();
				break; // We only want to loop if there are more tasks in the past	
			}
		}
	}
}

fn pinned_sleep(time_of_wake: Instant) {
	let mut guard = scheduler::SCHEDULER.lock();
	let sleep_state = SleepingTid {
		tid: guard.current_tid,
		time_of_wake
	};
	guard.add_to_sleep_queue(sleep_state);
	debug!("sleeping tid {:?}", guard.current_tid);
	recalculate_timer(&mut guard);
	guard.block(ThreadState::Blocked);
}

pub fn sleep(duration: Duration) {
	if duration <= Duration::from_secs(1) {
		// Core pinned sleep
		pinned_sleep(Instant::now() + duration);
	} else {
		todo!();
		push_to_global_sleep_queue(Instant::now() + duration);
	}
}

pub fn sleep_until(wake_time: Instant) {
	// to avoid having to calculate time until wake, always do a pinned sleep and have it pulled from back of queue later
	pinned_sleep(wake_time);
}

#[naked]
pub unsafe extern "C" fn thread_startup() {
	extern "C" fn thread_startup_inner() {
		unsafe {
			scheduler::SCHEDULER.unlock();
		}
	}

	asm!(
	"pop rbp", // aligns to 16 bytes
	"call {}",
	"pop rdi", // pop args off stack
	"pop rsi",
	"pop rdx",
	"pop rcx",
	"ret",
	sym thread_startup_inner, options(noreturn));
}

pub fn defer_schedule() {
	crate::hal::arch::apic::send_self_ipi(0x30);
}
