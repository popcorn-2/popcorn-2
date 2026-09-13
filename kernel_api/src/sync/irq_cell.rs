use core::cell::{Cell, UnsafeCell};
use core::fmt::{Debug, Formatter};
use core::marker::{PhantomData, Unsize};
use core::mem::ManuallyDrop;
use core::ops::{CoerceUnsized, Deref, DerefMut, DispatchFromDyn};

/// An interrupt safe, single-threaded mutable memory location.
///
/// This prevents deadlocks between interrupt handlers and main kernel code by
/// keeping interrupts disabled during the time a lock is held.
#[clippy::has_significant_drop]
pub struct IrqCell<T: ?Sized> {
	state: Cell<Option<usize>>,
	data: UnsafeCell<T>
}

impl<T> IrqCell<T> {
	/// Creates a new `IrqCell` containing `val`.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::sync::IrqCell;
	///
	/// let c = IrqCell::new(5);
	pub const fn new(val: T) -> Self {
		Self { state: Cell::new(None), data: UnsafeCell::new(val) }
	}
}

impl<T: ?Sized> IrqCell<T> {
	/// Locks the `IrqCell`, disabling interrupts on the current CPU.
	///
	/// # Panics
	///
	/// This function will panic if the `IrqCell` is already locked by the current CPU.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::sync::IrqCell;
	///
	/// let c = IrqCell::new(5);
	/// {
	///     let mut guard = c.lock();
	///     // Interrupts are now disabled to the end of this block
	///     *guard = 10;
	/// }
	///
	/// assert_eq!(*c.lock(), 10);
	/// ```
	///
	/// Causing a deadlock will cause a panic:
	///
	/// ```should_panic
	/// use kernel_api::sync::IrqCell;
	///
	/// let c = IrqCell::new(5);
	/// let x = c.lock();
	/// let y = c.lock();
	/// ```
	pub fn lock(&self) -> IrqGuard<'_, T> {
		// Unsafety: is this actually needed?
		assert!(self.state.get().is_none(), "IrqCell cannot be borrowed multiple times");

		self.state.set(Some(crate::bridge::irq::disable()));
		IrqGuard { cell: self, _phantom_not_send: PhantomData }
	}

	/// Creates a new [`IrqGuard`] without checking if the `IrqCell` is locked.
	///
	/// # Safety
	///
	/// The `IrqCell` must already be logically locked by the current CPU.
	///
	/// Only one `IrqGuard` must exist at any one time.
	pub unsafe fn make_guard_unchecked(&self) -> IrqGuard<'_, T> {
		// Unsafety: is this actually needed?
		debug_assert!(self.state.get().is_some(), "Created IrqGuard for unlocked IrqCell");

		IrqGuard { cell: self, _phantom_not_send: PhantomData }
	}

	/// Forcibly unlocks the `IrqCell`.
	///
	/// This is useful when combined with `mem::forget` to keep an `IrqCell`
	/// locked without holding onto a [`IrqGuard`].
	///
	/// # Safety
	///
	/// The current CPU must logically own an [`IrqGuard`] that has discarded in a way
	/// that prevents its [`Drop`] impl being called.
	pub unsafe fn unlock(&self) {
		let old_state = self.state.take();
		// SAFETY: `old_state` always exists while lock is held, which caller guarantees
		crate::bridge::irq::set(unsafe { old_state.unwrap_unchecked() });
	}
}

impl<T: Debug> Debug for IrqCell<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let guard = self.lock();
		f.debug_struct("IrqCell")
		 .field("data", &*guard)
		 .finish_non_exhaustive()?;
		IrqGuard::unlock(guard);
		Ok(())
	}
}

/// An RAII implementation of a "scoped lock" of an [`IrqCell`]. When this structure is
/// dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the `IrqCell` can be accessed through this guard via its
/// [`Deref`] and [`DerefMut`] implementations.
///
/// This structure is created by the [`lock`](`IrqCell::lock`) and
/// [`make_guard_unchecked`](`IrqCell::make_guard_unchecked`) methods on
/// [`IrqCell`].
pub struct IrqGuard<'cell, T: ?Sized> {
	cell: &'cell IrqCell<T>,
	_phantom_not_send: PhantomData<*mut u8>, // Dropping guard on other core would cause interrupts to be enabled in the wrong place
}

impl<T: ?Sized> IrqGuard<'_, T> {
	/// Unlocks the [`IrqCell`], keeping interrupts disabled regardless of their previous state.
	///
	///
	pub fn unlock_no_interrupts(this: Self) {
		let this = ManuallyDrop::new(this);
		this.cell.state.take();
	}

	/// Unlocks the [`IrqCell`], restoring interrupts to their previous state.
	///
	/// This is equivalent to dropping the `IrqGuard` but more explicit.
	fn unlock(this: Self) {
		drop(this);
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
		// SAFETY: Own IrqGuard therefore must have exclusive access to the data stored in the IrqCell
		unsafe { &*self.cell.data.get() }
	}
}

impl<T: ?Sized> DerefMut for IrqGuard<'_, T> {
	fn deref_mut(&mut self) -> &mut T {
		// SAFETY: Own IrqGuard therefore must have exclusive access to the data stored in the IrqCell
		unsafe { &mut *self.cell.data.get() }
	}
}

impl<T: ?Sized> Drop for IrqGuard<'_, T> {
	fn drop(&mut self) {
		// SAFETY: called from IrqGuard drop impl so will only be called once and caller must hold an IrqGuard
		unsafe { self.cell.unlock(); }
	}
}

impl<'a, T: ?Sized + Unsize<U>, U: ?Sized> DispatchFromDyn<IrqGuard<'a, U>> for IrqGuard<'a, T> {}
impl<'b, T: ?Sized + Unsize<U>, U: ?Sized> CoerceUnsized<IrqGuard<'b, U>> for IrqGuard<'b, T> {}
