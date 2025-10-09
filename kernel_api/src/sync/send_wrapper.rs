use core::mem::{ManuallyDrop, MaybeUninit};
use core::ptr::addr_of_mut;
use crate::address_space::WeakAddressSpace;

#[derive(Debug)]
pub struct SendWrapper<T> {
	inner: ManuallyDrop<T>,
	address_space: ManuallyDrop<WeakAddressSpace>,
}

unsafe impl<T> Send for SendWrapper<T> {}

impl<T> SendWrapper<T> {
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

	pub fn into_inner(self) -> T {
		let mut this = ManuallyDrop::new(self);
		this.into_inner_helper()
	}

	fn into_inner_helper(&mut self) -> T {
		let weak = unsafe { ManuallyDrop::take(&mut self.address_space) };
		let is_current = MaybeUninit::<bool>::uninit();
		let mut payload = (weak, is_current);
		crate::bridge::threading::with_current_thread(addr_of_mut!(payload).cast(), |meta, payload| {
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
