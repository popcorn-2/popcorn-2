#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::r#virtual::Global;
use crate::hal::{Hal, HalTy, SaveState};
use crate::hal::paging2::TTableTy;

#[derive(Debug)]
pub struct ThreadControlBlock {
	pub ttable: TTableTy,
	pub save_state: <HalTy as Hal>::SaveState,
	pub name: Cow<'static, str>,
	pub kernel_stack: Stack<'static, Global>,
	pub state: ThreadState,
}

impl ThreadControlBlock {
	pub fn new<Args: ArgTuple>(name: Cow<'static, str>, ttable: TTableTy, startup: unsafe extern "C" fn(), main: extern "C" fn(Args) -> !, args: Args) -> Self {
		let new_stack = Stack::new(
			mapping::Config::<Global>::new(NonZeroUsize::new(32).unwrap()),
			crate::paging_codes::THREAD_KERNEL_STACK,
		).unwrap();

		let mut new_thread = ThreadControlBlock {
			ttable,
			save_state: Default::default(),
			name,
			kernel_stack: new_stack,
			state: ThreadState::Ready,
		};
		let save_state = SaveState::new(&mut new_thread, startup, main, args.into_array());
		new_thread.save_state = save_state;

		new_thread
	}
}

impl Drop for ThreadControlBlock {
	fn drop(&mut self) {
		assert_ne!(self.state, ThreadState::Running, "Cannot drop currently running thread as this would remove the current stack");
	}
}

#[derive(Debug, PartialEq, Eq)]
pub enum ThreadState {
	Ready,
	Running,
	Blocked,
	Sleeping,
	AwaitingDeletion,
}

pub trait ArgTuple {
	fn into_array(self) -> [MaybeUninit<usize>; 4];
}

impl ArgTuple for () {
	fn into_array(self) -> [MaybeUninit<usize>; 4] {
		[MaybeUninit::uninit(), MaybeUninit::uninit(), MaybeUninit::uninit(), MaybeUninit::uninit()]
	}
}

impl ArgTuple for (usize,) {
	fn into_array(self) -> [MaybeUninit<usize>; 4] {
		[MaybeUninit::new(self.0), MaybeUninit::uninit(), MaybeUninit::uninit(), MaybeUninit::uninit()]
	}
}

impl ArgTuple for (usize,usize) {
	fn into_array(self) -> [MaybeUninit<usize>; 4] {
		[MaybeUninit::new(self.0), MaybeUninit::new(self.1), MaybeUninit::uninit(), MaybeUninit::uninit()]
	}
}

/*impl ArgTuple for (usize,usize,usize) {
	fn as_array(self) -> [MaybeUninit<usize>; 4] {
		[MaybeUninit::new(self.0), MaybeUninit::new(self.1), MaybeUninit::new(self.2), MaybeUninit::uninit()]
	}
}

impl ArgTuple for (usize,usize,usize,usize) {
	fn as_array(self) -> [MaybeUninit<usize>; 4] {
		[MaybeUninit::new(self.0), MaybeUninit::new(self.1), MaybeUninit::new(self.2), MaybeUninit::new(self.3)]
	}
}*/
