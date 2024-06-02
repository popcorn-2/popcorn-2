#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use core::arch::{asm, naked_asm};
use core::cmp::Ordering;
use core::num::NonZero;
use core::time::Duration;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::{highmem, OwnedFrames};
use kernel_api::memory::r#virtual::{Global, OwnedPages};
use kernel_api::time::Instant;
use crate::threading::tcb::{ThreadControlBlock, ThreadState};
use scheduler::Tid;

pub mod scheduler;
pub mod tcb;

pub unsafe fn init(handoff_data: crate::HandoffWrapper) -> Tid {
	let stack = handoff_data.memory.stack;
	let ttable = handoff_data.to_empty_ttable();

	// fixme: is highmem always correct?
	let stack_phys_len = stack.top_virt - stack.bottom_virt - 1;
	let stack_frames = OwnedFrames::from_raw_parts(
		stack.top_phys - stack_phys_len,
		NonZero::<usize>::new(stack_phys_len).expect("Cannot have a zero sized stack"),
		highmem()
	);
	let stack_pages = OwnedPages::from_raw_parts(
		stack.bottom_virt,
		NonZero::<usize>::new(stack_phys_len + 1).expect("Cannot have a zero sized stack"),
		Global
	);

	let mut scheduler = scheduler::SCHEDULER.lock();
	let tcb =  ThreadControlBlock {
		name: Cow::Borrowed("init"),
		kernel_stack: Stack::from_contiguous_raw_parts(stack_frames, stack_pages),
		ttable,
		state: ThreadState::Running,
		save_state: Default::default(),
	};
	
	scheduler.init(tcb);

	Tid(0)
}

pub fn thread_yield() {
	defer_schedule();
}

pub fn block(reason: ThreadState) {
	scheduler::SCHEDULER.lock().block(reason);
}

pub fn current_thread() -> Tid {
	scheduler::SCHEDULER.lock().current_tid()
}

pub fn unblock(tid: Tid) {
	scheduler::SCHEDULER.lock().unblock(tid);
}

#[derive(Debug, Copy, Clone)]
struct SchedulerEvent {
	time: Instant,
	tid: Tid,
	action: EventTy,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum EventTy {
	Unblock,
}

impl Ord for SchedulerEvent {
	fn cmp(&self, other: &Self) -> Ordering {
		self.time.cmp(&other.time)
	}
}

impl PartialOrd for SchedulerEvent {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl PartialEq for SchedulerEvent {
	fn eq(&self, other: &Self) -> bool {
		self.time.eq(&other.time)
	}
}

impl Eq for SchedulerEvent {}

fn push_to_global_sleep_queue(_wake_time: Instant) {
	todo!()
}

fn pinned_sleep(time_of_wake: Instant) {
	let mut guard = scheduler::SCHEDULER.lock();
	let sleep_event = SchedulerEvent {
		tid: guard.current_tid(),
		time: time_of_wake,
		action: EventTy::Unblock
	};
	#[cfg(feature = "log.scheduler")] debug!("sleeping tid {:?}", sleep_event.tid);
	guard.event_queue.add(sleep_event);
	guard.block(ThreadState::Sleeping);
}

pub fn sleep(duration: Duration) {
	// fixme: if duration <= Duration::from_secs(1) {
		// Core pinned sleep
		pinned_sleep(Instant::now() + duration);
	//} else {
	//	todo!();
	//	push_to_global_sleep_queue(Instant::now() + duration);
	//}
}

pub fn sleep_until(wake_time: Instant) {
	// to avoid having to calculate time until wake, always do a pinned sleep and have it pulled from back of queue later
	pinned_sleep(wake_time);
}

pub fn exit(exit_code: i8) -> ! {
	let mut guard = scheduler::SCHEDULER.lock();
	guard.queue_for_deletion(exit_code);
	drop(guard); // drop guard before end of scope to ensure deferred schedule goes through
	unreachable!("Returned to deleted task")
}

#[naked]
pub unsafe extern "C" fn thread_startup() {
	extern "C" fn thread_startup_inner() {
		unsafe {
			scheduler::SCHEDULER.unlock();
		}
	}

	naked_asm!(
		".cfi_startproc simple",
		".cfi_def_cfa rsp, 48",
		".cfi_offset rip, -48",
		"pop rbp", // aligns to 16 bytes
		".cfi_def_cfa rsp, 40",
		".cfi_register rip, rbp",
		"call {}",
		"pop rdi", // pop args off stack
		".cfi_def_cfa rsp, 32",
		"pop rsi",
		".cfi_def_cfa rsp, 24",
		"pop rdx",
		".cfi_def_cfa rsp, 16",
		"pop rcx",
		".cfi_def_cfa rsp, 8",
		"ret",
		".cfi_endproc",
	sym thread_startup_inner);
}

pub fn defer_schedule() {
	crate::hal::arch::apic::send_self_ipi(0x30);
}
