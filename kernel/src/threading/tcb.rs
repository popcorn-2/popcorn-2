#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::r#virtual::Global;
use crate::hal::{Hal, HalTy, SaveState};
use crate::hal::paging2::TTableTy;

macro_rules! gen_field {
    /* mutable - thread */ (ref Thread, Thread, $field_ty:ty) => { &'a mut $field_ty };
    /* mutable - pointer */ (ref Pointer, Pointer, $field_ty:ty) => { &'a mut $field_ty };
	/* invisible */ (ref $_: ident, $_1: ident, $field_ty:ty) => { () };
	/* immutable */ (ref $_: ident, $field_ty:ty) => { &'a $field_ty };
    /* mutable - underlying */ ($_: ident, $field_ty:ty) => { ::core::cell::UnsafeCell<$field_ty> };
	/* immutable - underlying */ ($field_ty:ty) => { $field_ty };
}

macro_rules! extract_field {
    /* mutable */ ($tcb:ident, ref Thread, Thread, $field_name: ident) => { unsafe { &mut *$tcb.$field_name.get() } };
    /* mutable */ ($tcb:ident, ref Pointer, Pointer, $field_name: ident) => { unsafe { &mut *$tcb.$field_name.get() } };
	/* invisible */ ($tcb:ident, ref $_: ident, $_1: ident, $field_name: ident) => { () };
	/* immutable */ ($tcb:ident, ref $_: ident, $field_name: ident) => { &$tcb.$field_name };
}

macro_rules! tcb_views {
    (pub struct ThreadControlBlock {
	    $($(#[mut = $field_place: ident])? $field_name: ident : $field_ty:ty),* $(,)?
    }) => {
		#[derive(Debug)]
	    pub struct ThreadControlBlock {
		    $(pub $field_name : gen_field!( $($field_place ,)? $field_ty)),*
	    }

		#[derive(Debug)]
	    pub struct OwnedView<'a> {
		    $(pub $field_name : gen_field!( ref Thread, $($field_place ,)? $field_ty)),*
	    }

		#[derive(Debug)]
	    pub struct PointerView<'a> {
		    $(pub $field_name : gen_field!( ref Pointer, $($field_place ,)? $field_ty)),*
	    }

	    impl ThreadControlBlock {
		    fn new_inner($($field_name: $field_ty),*) -> ThreadControlBlock {
			    ThreadControlBlock {
				    $($field_name : ::core::convert::Into::into($field_name)),*
			    }
		    }
	    }

	    impl OwnedView<'_> {
		    /// # Safety
		    ///
		    /// No other `OwnedView`s to the same [`ThreadControlBlock`] must exist at the same time
		    unsafe fn from_tcb(tcb: &ThreadControlBlock) -> OwnedView<'_> {
			    OwnedView {
				    $($field_name : extract_field!( tcb, ref Thread, $($field_place ,)? $field_name)),*
			    }
		    }
	    }

	    impl PointerView<'_> {
		    /// # Safety
		    ///
		    /// No other `PointerView`s to the same [`ThreadControlBlock`] must exist at the same time
		    unsafe fn from_tcb(tcb: &ThreadControlBlock) -> PointerView<'_> {
			    PointerView {
				    $($field_name : extract_field!( tcb, ref Pointer, $($field_place ,)? $field_name)),*
			    }
		    }
	    }
    };
}

tcb_views! {
	pub struct ThreadControlBlock {
		ttable: TTableTy,
		#[mut = Pointer] save_state: <HalTy as Hal>::SaveState,
		name: Cow<'static, str>,
		kernel_stack: Stack<'static, Global>,
		state: ThreadState,
	}
}

impl ThreadControlBlock {
	pub fn new<Args: ArgTuple>(name: Cow<'static, str>, ttable: TTableTy, startup: unsafe extern "C" fn(), main: extern "C" fn(Args) -> !, args: Args) -> Self {
		let new_stack = Stack::new(
			mapping::Config::<Global>::new(NonZeroUsize::new(32).unwrap()),
			crate::paging_codes::THREAD_KERNEL_STACK,
		).unwrap();

		let mut new_thread = ThreadControlBlock::new_inner(
			ttable,
			Default::default(),
			name,
			new_stack,
			ThreadState::Ready,
		);
		new_thread.save_state = UnsafeCell::new(SaveState::new(&mut new_thread, startup, main, args.into_array()));

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
