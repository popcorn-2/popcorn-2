use alloc::boxed::Box;
use core::fmt::{Debug, Formatter};
use core::ptr::NonNull;
use super::{PointerView, SharedView, OwnedView, ThreadControlBlock};

/// Global ownership of a [`ThreadControlBlock`]
///
/// See the [module level documentation](crate::threading#threadcontrolblock-vs-thread-vs-threadpointer-vs-threadid) for more information
pub struct Thread {
	pub(super) ptr: NonNull<ThreadControlBlock>,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
	/// Constructs an Owned pointer to a [`ThreadControlBlock`]. See the [module level docs](self) for more information.
	pub fn new(tcb: ThreadControlBlock) -> Thread {
		let b = Box::new(tcb);
		let ptr = NonNull::from(Box::leak(b));
		Thread { ptr }
	}

	/// Immutably "borrows" a [`Thread`]
	pub fn tcb_ref(&self) -> SharedView<'_> {
		let tcb = unsafe { self.ptr.as_ref() };
		SharedView::from_tcb(tcb)
	}

	/// Mutably "borrows" a [`Thread`]
	pub fn tcb_mut(&mut self) -> OwnedView<'_> {
		let tcb = unsafe { self.ptr.as_ref() };
		unsafe { OwnedView::from_tcb(tcb) }
	}
}

/// Scheduler ownership of a [`ThreadControlBlock`]
///
/// See the [module level documentation](crate::threading#threadcontrolblock-vs-thread-vs-threadpointer-vs-threadid) for more information
/// 
/// This is guaranteed to have the same layout as [`NonNull<ThreadControlBlock>`].
#[repr(transparent)]
pub struct ThreadPointer {
	ptr: NonNull<ThreadControlBlock>,
}

unsafe impl Send for ThreadPointer {}
unsafe impl Sync for ThreadPointer {}

impl ThreadPointer {
	/// # Safety
	///
	/// The caller must ensure that no other [`ThreadPointer`]s to the same [`ThreadControlBlock`] exist
	pub unsafe fn new_unchecked(owned: &Thread) -> ThreadPointer {
		ThreadPointer { ptr: owned.ptr }
	}

	pub fn new(owned: Thread) -> (Thread, ThreadPointer) {
		let ptr = unsafe { Self::new_unchecked(&owned) };
		(owned, ptr)
	}

	/// Immutably "borrows" a [`ThreadPointer`]
	pub fn tcb_ref(&self) -> SharedView<'_> {
		let tcb = unsafe { self.ptr.as_ref() };
		SharedView::from_tcb(tcb)
	}

	/// Mutably "borrows" a [`ThreadPointer`]
	pub fn tcb_mut(&mut self) -> PointerView<'_> {
		let tcb = unsafe { self.ptr.as_ref() };
		unsafe { PointerView::from_tcb(tcb) }
	}
}

impl Debug for Thread {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Thread")
		 .field("id", self.tcb_ref().thread_id)
		 .field("state", self.tcb_ref().state)
		 .field("name", self.tcb_ref().name)
		 .finish_non_exhaustive()
	}
}

impl Debug for ThreadPointer {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("ThreadPointer")
		 .field("id", self.tcb_ref().thread_id)
		 .field("state", self.tcb_ref().state)
		 .field("name", self.tcb_ref().name)
		 .finish_non_exhaustive()
	}
}
