use core::mem::{ManuallyDrop, MaybeUninit};
use crate::address_space::WeakAddressSpace;

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
	address_space: ManuallyDrop<WeakAddressSpace>,
}

// SAFETY: No methods allow interaction with the inner type unless called from the original address space
#[expect(clippy::non_send_fields_in_send_ty, reason = "this type is designed for safely sending non-Send types")]
unsafe impl<T> Send for SendWrapper<T> {}

impl<T> SendWrapper<T> {
	/// Creates a wrapper around `inner` to allow it to be sent to another thread.
	pub fn new(inner: T) -> Self {
		let mut address_space = MaybeUninit::<WeakAddressSpace>::uninit();
		crate::bridge::threading::with_current_thread(address_space.as_mut_ptr().cast(), |meta, out| {
			unsafe { core::ptr::write(out.cast::<WeakAddressSpace>(), meta.address_space.downgrade()) };
		});
		Self {
			inner: ManuallyDrop::new(inner),
			address_space: ManuallyDrop::new(unsafe { address_space.assume_init() }),
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
		this.into_inner_helper()
	}

	#[expect(clippy::wrong_self_convention, reason = "needs to be callable from drop impl")]
	fn into_inner_helper(&mut self) -> T {
		let weak = unsafe { ManuallyDrop::take(&mut self.address_space) };
		let is_current = MaybeUninit::<bool>::uninit();
		let mut payload = (weak, is_current);
		crate::bridge::threading::with_current_thread((&raw mut payload).cast(), |meta, payload| {
			let payload = unsafe { &mut *payload.cast::<(WeakAddressSpace, MaybeUninit<bool>)>() };
			let is_current = WeakAddressSpace::ptr_eq(&payload.0, &meta.address_space);
			payload.1.write(is_current);
		});

		if unsafe { payload.1.assume_init() } {
			unsafe { ManuallyDrop::take(&mut self.inner) }
		} else {
			panic!("attempted to drop or unwrap `SendWrapper` in wrong address space")
		}
	}
}

impl<T> Drop for SendWrapper<T> {
	fn drop(&mut self) {
		self.into_inner_helper();
	}
}
