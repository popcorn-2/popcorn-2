use alloc::borrow::Cow;
use core::arch::asm;
use core::num::NonZeroUsize;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::{highmem, OwnedFrames};
use kernel_api::memory::r#virtual::{Global, OwnedPages};
use crate::hal::paging2::TTableTy;
use crate::hal::{ThreadControlBlock, ThreadState};
use utils::handoff;
use scheduler::Tid;

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

	Tid(0)
}

pub fn thread_yield() {
	scheduler::SCHEDULER.lock().schedule();
}

pub fn thread_block(reason: ThreadState) {
	scheduler::SCHEDULER.lock().block(reason);
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
