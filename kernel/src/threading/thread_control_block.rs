use alloc::borrow::Cow;
use alloc::sync::Arc;
use core::cell::UnsafeCell;
use core::fmt::{Debug, Formatter};
use core::num::{NonZero, NonZeroUsize};
use core::ptr;
use core::sync::atomic::{AtomicU128, Ordering};
use kernel_api::address_space::{AddressSpace, Kernel};
use kernel_api::mapping::{Config, Mapping, Stack};
use kernel_api::threading::{ParkGaurd, ThreadId};
use crate::hal::SaveState;
use super::{Thread, ThreadPointer, WakeReason};
use crate::hal::SaveStateTr;
use crate::ipc::handle::HandleMap;
use crate::threading::subthread_killer::SubthreadKiller;
use crate::ipc::async_handling::AsyncMap;

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
		/// The address space for the thread
		address_space: AddressSpace,
		/// The saved CPU state
		#mut(Pointer) save_state: SaveState,
		/// The user-facing name of the thread
		name: Cow<'static, str>,
		/// The stack that kernel code runs on inside the thread
		kernel_stack: Mapping<Stack, Kernel>,
		/// The current running state of the thread
		state: AtomicThreadState,
		/// The numerical ID of the thread
		thread_id: ThreadId,
		/// The currently open handles
		handles: Arc<HandleMap>,
		subthread_killer: SubthreadKiller,
		async_map: Arc<AsyncMap>,
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
	/// extern "C" fn my_thread(a: usize) -> ! {
	///     assert_eq!(a, 1);
	///     loop {}
	/// }
	///
	/// # let current_ttable = TTableTy::new(&*crate::memory::paging::ktable(), kernel_api::memory::physical::highmem()).unwrap();
	/// let (tcb, id) = ThreadControlBlock::new(
	///     "My new thread".into(),
	///     current_ttable,
	///     kernel::threading::thread_startup,
	///     my_thread,
	///     1,
	/// );
	/// 
	/// enqueue_new(tcb);
	/// ```
	pub fn new(name: Cow<'static, str>, address_space: AddressSpace, handles: HandleMap, startup: unsafe extern "C" fn(), main: extern "C" fn(usize) -> !, arg: usize, id: Option<ThreadId>) -> (Self, ThreadId) {
		let id = id.unwrap_or_else(ThreadId::new);

		let new_stack = Config::new(NonZeroUsize::new(32).unwrap())
				.protection(true, false, false)
				.map()
				.expect("failed to allocate stack");
		
		let mut new_thread = ThreadControlBlock::new_inner(
			address_space,
			Default::default(),
			name,
			new_stack,
			AtomicThreadState::new(ThreadState::Ready),
			id,
			Arc::new(handles),
			SubthreadKiller::new(id),
			Arc::new(AsyncMap::new()),
		);
		new_thread.save_state = UnsafeCell::new(SaveState::new(&mut new_thread, startup, main, arg));

		(new_thread, id)
	}

	pub fn clone_from(from: SharedView, name: Cow<'static, str>, startup: unsafe extern "C" fn(), main: extern "C" fn(usize) -> !, arg: usize) -> (Self, ThreadId) {
		let new_stack = Config::new(NonZeroUsize::new(32).unwrap())
				.protection(true, false, false)
				.map()
				.expect("failed to allocate stack");

		let id = ThreadId::new();
		let killer = SubthreadKiller::clone(from.subthread_killer);
		killer.add_thread(id);
		let mut new_thread = ThreadControlBlock::new_inner(
			Arc::clone(from.address_space),
			Default::default(),
			name,
			new_stack,
			AtomicThreadState::new(ThreadState::Ready),
			id,
			Arc::clone(from.handles),
			killer,
			Arc::clone(from.async_map),
		);
		new_thread.save_state = UnsafeCell::new(SaveState::new(&mut new_thread, startup, main, arg));

		(new_thread, id)
	}
	
	pub fn from_owned(thread: Thread, _pointer: ThreadPointer) -> Self {
		unsafe { *Box::from_raw(thread.ptr.as_ptr()) }
	}
}

impl Drop for ThreadControlBlock {
	fn drop(&mut self) {
		debug!("dropped TCB for thread {:?}", self.thread_id);
		assert!(!self.state.load(Ordering::SeqCst).is_running(), "Cannot drop currently running thread as this would remove the current stack");
	}
}

pub struct AtomicThreadState(AtomicU128);

impl AtomicThreadState {
	fn to_raw(state: ThreadState) -> u128 {
		let (high, low): (usize, usize) = match state {
			ThreadState::Ready => (0, 0),
			ThreadState::Running => (1, 0),
			ThreadState::Parked(park) => (2, Arc::into_raw(park).expose_provenance()),
			ThreadState::JustUnparked(WakeReason::Timeout) => (3, 0),
			ThreadState::JustUnparked(WakeReason::Custom(val)) => (3, val.get() as usize),
			ThreadState::NearlyParked(park) => (4, Arc::into_raw(park).expose_provenance()),
			ThreadState::Dead => (5, 0),
		};

		(high as u128) << 64 | (low as u128)
	}

	fn from_raw(val: u128) -> ThreadState {
		let (high, low) = ((val >> 64) as usize, val as usize);

		match high {
			0 => ThreadState::Ready,
			1 => ThreadState::Running,
			2 => {
				let arc = unsafe { Arc::from_raw(ptr::with_exposed_provenance(low)) };
				// since we store an arc ourselves
				unsafe { Arc::increment_strong_count(Arc::as_ptr(&arc)) };
				ThreadState::Parked(arc)
			},
			3 => ThreadState::JustUnparked(match NonZero::new(low as u16) {
				None => WakeReason::Timeout,
				Some(val) => WakeReason::Custom(val),
			}),
			4 => {
				let arc = unsafe { Arc::from_raw(ptr::with_exposed_provenance(low)) };
				// since we store an arc ourselves
				unsafe { Arc::increment_strong_count(Arc::as_ptr(&arc)) };
				ThreadState::NearlyParked(arc)
			},
			5 => ThreadState::Dead,
			_ => unreachable!()
		}
	}

	pub fn new(state: ThreadState) -> Self {
		AtomicThreadState(AtomicU128::new(Self::to_raw(state)))
	}

	pub fn store(&self, state: ThreadState, ordering: Ordering) {
		self.0.store(Self::to_raw(state), ordering);
	}

	pub fn load(&self, ordering: Ordering) -> ThreadState {
		let val = self.0.load(ordering);
		Self::from_raw(val)
	}
	
	pub fn compare_exchange(&self, current: ThreadState, new: ThreadState, success: Ordering, failure: Ordering) -> Result<(), ()> {
		let current = Self::to_raw(current);
		let new = Self::to_raw(new);
		self.0.compare_exchange(current, new, success, failure)
				.map(|_| ())
				.map_err(|_| ())
	}

	pub unsafe fn fetch_update_raw(&self, set_order: Ordering, fetch_order: Ordering, f: impl FnMut(u128) -> Option<u128>) {
		self.0.fetch_update(set_order, fetch_order, f);
	}
}

impl Debug for AtomicThreadState {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		Debug::fmt(&self.load(Ordering::Relaxed), f)
	}
}

#[derive(Debug)]
pub enum ThreadState {
	/// The thread is able to run, but has not yet been scheduled
	Ready,
	/// The thread is actively running
	Running,
	/// The thread is parked
	Parked(Arc<ParkGaurd>),
	/// The thread is in the process of being parked
	NearlyParked(Arc<ParkGaurd>),
	/// The thread was unparked but has not been run since
	JustUnparked(WakeReason),
	Dead,
}

impl ThreadState {
	pub fn is_ready(&self) -> bool {
		match self {
			Self::Ready => true,
			Self::JustUnparked(_) => true,
			Self::NearlyParked(_) => true,
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
