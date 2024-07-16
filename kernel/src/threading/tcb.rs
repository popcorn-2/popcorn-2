#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::r#virtual::Global;
use crate::hal::{Hal, HalTy, SaveState};
use crate::hal::paging2::TTableTy;
use crate::threading::{ParkState, ThreadId, WakeReason};

#[doc(hidden)]
macro_rules! __tcb_gen_field {
    /* mutable - thread */ (ref Thread, Thread, $field_ty:ty) => { &'a mut $field_ty };
    /* mutable - pointer */ (ref Pointer, Pointer, $field_ty:ty) => { &'a mut $field_ty };
	/* invisible */ (ref $_: ident, $_1: ident, $field_ty:ty) => { () };
	/* immutable */ (ref $_: ident, $field_ty:ty) => { &'a $field_ty };
    /* mutable - underlying */ ($_: ident, $field_ty:ty) => { ::core::cell::UnsafeCell<$field_ty> };
	/* immutable - underlying */ ($field_ty:ty) => { $field_ty };
}

#[doc(hidden)]
macro_rules! __tcb_extract_field {
    /* mutable */ ($tcb:ident, ref Thread, Thread, $field_name: ident) => { unsafe { &mut *$tcb.$field_name.get() } };
    /* mutable */ ($tcb:ident, ref Pointer, Pointer, $field_name: ident) => { unsafe { &mut *$tcb.$field_name.get() } };
	/* invisible */ ($tcb:ident, ref $_: ident, $_1: ident, $field_name: ident) => { () };
	/* immutable */ ($tcb:ident, ref $_: ident, $field_name: ident) => { &$tcb.$field_name };
}

#[doc(hidden)]
macro_rules! tcb_views {
    (pub struct ThreadControlBlock {
	    $($(#[$attr:meta])* $(#mut($field_place: ident))? $field_name: ident : $field_ty:ty),* $(,)?
    }) => {
	    /// The underlying state of a thread
		#[derive(Debug)]
	    pub struct ThreadControlBlock {
		    $($(#[$attr])* pub $field_name : __tcb_gen_field!( $($field_place ,)? $field_ty)),*
	    }

	    /// A mutable "borrow" of [`ThreadControlBlock`] when "borrowed" from a [`Thread`](super::Thread)
		#[derive(Debug)]
	    pub struct OwnedView<'a> {
		    $($(#[$attr])* pub $field_name : __tcb_gen_field!( ref Thread, $($field_place ,)? $field_ty)),*
	    }

	    /// A mutable "borrow" of [`ThreadControlBlock`] when "borrowed" from a [`ThreadPointer`](super::ThreadPointer)
		#[repr(C)]
		#[derive(Debug)]
	    pub struct PointerView<'a> {
		    $($(#[$attr])* pub $field_name : __tcb_gen_field!( ref Pointer, $($field_place ,)? $field_ty)),*
	    }

	    /// An immutable "borrow" of [`ThreadControlBlock`] when "borrowed" from a [`Thread`](super::Thread) or [`ThreadPointer`](super::ThreadPointer)
		#[repr(C)]
		#[derive(Debug)]
	    pub struct SharedView<'a> {
		    $($(#[$attr])* pub $field_name : __tcb_gen_field!( ref Shared, $($field_place ,)? $field_ty)),*
	    }

	    impl ThreadControlBlock {
		    /// A constructor function that internally converts mutable fields to `UnsafeCell`s
		    pub(super) fn new_inner($($field_name: $field_ty),*) -> ThreadControlBlock {
			    ThreadControlBlock {
				    $($field_name : ::core::convert::Into::into($field_name)),*
			    }
		    }
	    }

	    impl OwnedView<'_> {
		    /// Creates a mutable [`OwnedView`] from the underlying [`ThreadControlBlock`]
		    ///
		    /// # Safety
		    ///
		    /// No other `OwnedView`s to the same [`ThreadControlBlock`] must exist at the same time
		    pub(super) unsafe fn from_tcb(tcb: &ThreadControlBlock) -> OwnedView<'_> {
			    OwnedView {
				    $($field_name : __tcb_extract_field!( tcb, ref Thread, $($field_place ,)? $field_name)),*
			    }
		    }
	    }

	    impl PointerView<'_> {
		    /// Creates a mutable [`PointerView`] from the underlying [`ThreadControlBlock`]
		    ///
		    /// # Safety
		    ///
		    /// No other `PointerView`s to the same [`ThreadControlBlock`] must exist at the same time
		    pub(super) unsafe fn from_tcb(tcb: &ThreadControlBlock) -> PointerView<'_> {
			    PointerView {
				    $($field_name : __tcb_extract_field!( tcb, ref Pointer, $($field_place ,)? $field_name)),*
			    }
		    }
	    }

	    impl SharedView<'_> {
		    /// Creates an immutable [`SharedView`] from the underlying [`ThreadControlBlock`]
		    pub(super) fn from_tcb(tcb: &ThreadControlBlock) -> SharedView<'_> {
			    SharedView {
				    $($field_name : __tcb_extract_field!( tcb, ref Shared, $($field_place ,)? $field_name)),*
			    }
		    }
	    }
    };
}

tcb_views! {
	pub struct ThreadControlBlock {
		/// The page table for the thread
		ttable: TTableTy,
		/// The saved CPU state
		#mut(Pointer) save_state: <HalTy as Hal>::SaveState,
		/// The user-facing name of the thread
		name: Cow<'static, str>,
		/// The stack that kernel code runs on inside the thread
		kernel_stack: Stack<'static, Global>,
		/// The current running state of the thread
		#mut(Pointer) state: ThreadState,
		/// The numerical ID of the thread
		thread_id: ThreadId,
	}
}

impl ThreadControlBlock {
	/// Creates the [`ThreadControlBlock`] for a new thread
	///
	/// Assigns a [`ThreadId`] to the thread, and initializes the [`SaveState`](Hal::SaveState) to call `main`
	/// with the arguments given in the `args` tuple
	///
	/// # Panics
	///
	/// Panics if a stack cannot be created for the new thread
	///
	/// # Examples
	///
	/// ```
	/// use kernel::threading::tcb::ThreadControlBlock;
	/// use kernel::threading::scheduler::enqueue_new;
	/// # use kernel::prelude::debug;
	///
	/// extern "C" fn my_thread((a, b): (usize, usize)) -> ! {
	///     assert_eq!(a, 1);
	///     assert_eq!(b, 2);
	///     loop {}
	/// }
	///
	/// # let current_ttable = TTableTy::new(&*crate::memory::paging::ktable(), kernel_api::memory::physical::highmem()).unwrap();
	/// let (tcb, id) = ThreadControlBlock::new(
	///     "My new thread".into(),
	///     current_ttable,
	///     kernel::threading::thread_startup,
	///     my_thread,
	///     (1, 2),
	/// );
	/// 
	/// enqueue_new(tcb);
	/// ```
	pub fn new<Args: ArgTuple>(name: Cow<'static, str>, ttable: TTableTy, startup: unsafe extern "C" fn(), main: extern "C" fn(Args) -> !, args: Args) -> (Self, ThreadId) {
		let new_stack = Stack::new(
			mapping::Config::<Global>::new(NonZeroUsize::new(32).unwrap()),
			crate::paging_codes::THREAD_KERNEL_STACK,
		).unwrap();
		
		let id = ThreadId::new();
		let mut new_thread = ThreadControlBlock::new_inner(
			ttable,
			Default::default(),
			name,
			new_stack,
			ThreadState::Ready,
			id,
		);
		new_thread.save_state = UnsafeCell::new(SaveState::new(&mut new_thread, startup, main, args.into_array()));

		(new_thread, id)
	}
}

impl Drop for ThreadControlBlock {
	fn drop(&mut self) {
		assert!(!self.state.get_mut().is_running(), "Cannot drop currently running thread as this would remove the current stack");
	}
}

#[derive(Debug)]
pub enum ThreadState {
	/// The thread is able to run, but has not yet been scheduled
	Ready,
	/// The thread is actively running
	Running,
	/// The thread is parked
	Parked(Arc<ParkState>),
	/// The thread was unparked but has not been run since
	JustUnparked(WakeReason),
}

impl ThreadState {
	pub fn is_ready(&self) -> bool {
		match self {
			Self::Ready => true,
			Self::JustUnparked(_) => true,
			_ => false,
		}
	}

	pub fn is_running(&self) -> bool {
		match self {
			Self::Running => true,
			_ => false,
		}
	}

	pub fn is_parked(&self) -> bool {
		match self {
			Self::Parked(_) => true,
			_ => false,
		}
	}

	pub fn is_just_unparked(&self) -> bool {
		match self {
			Self::JustUnparked(_) => true,
			_ => false,
		}
	}
	
	pub fn wake_reason(&self) -> Option<WakeReason> {
		match self {
			Self::JustUnparked(reason) => Some(*reason),
			_ => None,
		}
	}
}

/// Implementation detail of [`ThreadControlBlock::new()`] to work around the lack of variadic generics
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
