use core::mem::ManuallyDrop;
use crate::address_space;

/// Implements [`Send`] for the contained type by preventing access except from the address space it was created in.
///
/// This can be used to allow a type to implement `Send` and thus be sent between threads, while containing
/// non-`Send` fields, as long as the container is eventually sent back to the original thread before being accessed
/// or dropped.
///
/// # Panics
///
/// Will panic if not dropped in the address space it was created in.
#[derive(Debug)]
pub struct SendWrapper<T> {
	inner: ManuallyDrop<T>,
	address_space: ManuallyDrop<address_space::Weak>,
}

// SAFETY: No methods allow interaction with the inner type unless called from the original address space
#[expect(clippy::non_send_fields_in_send_ty, reason = "this type is designed for safely sending non-Send types")]
unsafe impl<T> Send for SendWrapper<T> {}

impl<T> SendWrapper<T> {
	/// Creates a wrapper around `inner` to allow it to be sent to another thread.
	pub fn new(inner: T) -> Self {
		let address_space = crate::bridge::threading::with_current_thread(|meta| {
			meta.address_space.downgrade()
		});

		Self {
			inner: ManuallyDrop::new(inner),
			address_space: ManuallyDrop::new(address_space),
		}
	}

	/// Consumes `self`, returning the contained value.
	///
	/// # Panics
	///
	/// This method panics if not called on the same thread as the `SendWrapper`
	/// was created on.
	pub fn into_inner(self) -> T {
		let mut this = ManuallyDrop::new(self);
		// SAFETY: function consumes `self` so can only be called once during lifetime
		unsafe { this.into_inner_helper() }
	}

	/// # Safety
	///
	/// Must only be called on `self` once.
	///
	/// # Panics
	///
	/// If called from a different address space to where `self` was constructed.
	#[expect(clippy::wrong_self_convention, reason = "needs to be callable from drop impl")]
	unsafe fn into_inner_helper(&mut self) -> T {
		// SAFETY: requirements of `ManuallyDrop::take` upheld by caller
		let weak = unsafe { ManuallyDrop::take(&mut self.address_space) };

		let res = crate::bridge::threading::with_current_thread(|meta| {
			address_space::Weak::ptr_eq(&weak, &meta.address_space)
		});

		if res {
			// SAFETY: requirements of `ManuallyDrop::take` upheld by caller, and
			// checked if we're calling drop in the right address space
			unsafe { ManuallyDrop::take(&mut self.inner) }
		} else {
			panic!("attempted to drop or unwrap `SendWrapper` in wrong address space")
		}
	}
}

impl<T> Drop for SendWrapper<T> {
	fn drop(&mut self) {
		// SAFETY: drop impl is only called once during lifetime
		unsafe { self.into_inner_helper() };
	}
}
