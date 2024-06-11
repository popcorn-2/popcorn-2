#![unstable(feature = "kernel_irq_cell", issue = "none")]

use core::cell::{Cell, UnsafeCell};
use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};

pub struct IrqCell<T: ?Sized> {
	state: Cell<Option<usize>>,
	data: UnsafeCell<T>
}

impl<T> IrqCell<T> {
	pub const fn new(val: T) -> Self {
		Self { state: Cell::new(None), data: UnsafeCell::new(val) }
	}
}

impl<T: ?Sized> IrqCell<T> {
	pub fn lock(&self) -> IrqGuard<'_, T> {
		// Unsafety: is this actually needed?
		if self.state.get().is_some() { panic!("IrqCell cannot be borrowed multiple times"); }

		self.state.set(Some(unsafe { crate::bridge::hal::__popcorn_disable_irq() }));
		IrqGuard { cell: self, _phantom_not_send: PhantomData }
	}

	pub unsafe fn make_guard_unchecked(&self) -> IrqGuard<'_, T> {
		// Unsafety: is this actually needed?
		debug_assert!(self.state.get().is_some(), "Created IrqGuard for unlocked IrqCell");

		IrqGuard { cell: self, _phantom_not_send: PhantomData }
	}

	pub unsafe fn unlock(&self) {
		let old_state = self.state.take();
		unsafe { crate::bridge::hal::__popcorn_set_irq(old_state.unwrap()); }
	}
}

pub struct IrqGuard<'cell, T: ?Sized> {
	cell: &'cell IrqCell<T>,
	_phantom_not_send: PhantomData<*mut u8>, // Dropping guard on other core would cause interrupts to be enabled in the wrong place
}

impl<T: ?Sized> IrqGuard<'_, T> {
	pub fn unlock_no_interrupts(this: IrqGuard<T>) {
		let this = ManuallyDrop::new(this);
		this.cell.state.take();
	}
}

impl<T: Debug> Debug for IrqGuard<'_, T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("IrqGuard")
		 .field("cell", &**self)
		 .finish()
	}
}

impl<T: ?Sized> Deref for IrqGuard<'_, T> {
	type Target = T;

	fn deref(&self) -> &T {
		unsafe { &*self.cell.data.get() }
	}
}

impl<T: ?Sized> DerefMut for IrqGuard<'_, T> {
	fn deref_mut(&mut self) -> &mut T {
		unsafe { &mut *self.cell.data.get() }
	}
}

impl<T: ?Sized> Drop for IrqGuard<'_, T> {
	fn drop(&mut self) {
		unsafe { self.cell.unlock(); }
	}
}
