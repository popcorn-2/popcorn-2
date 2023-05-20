use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};

pub struct LateInit<T> {
	data: MaybeUninit<T>,
	initialised: bool
}

impl<T> LateInit<T> {
	pub const fn new() -> Self {
		Self {
			data: MaybeUninit::uninit(),
			initialised: false
		}
	}

	pub fn init(&mut self, val: T) -> &mut T {
		self.data.write(val);
		self.initialised = true;
		self
	}
}

impl<T> const Deref for LateInit<T> {
	type Target = T;

	#[track_caller]
	fn deref(&self) -> &Self::Target {
		assert!(self.initialised, "Tried to access uninitialised LateInit");
		unsafe { self.data.assume_init_ref() }
	}
}

impl<T> DerefMut for LateInit<T> {
	#[track_caller]
	fn deref_mut(&mut self) -> &mut Self::Target {
		assert!(self.initialised, "Tried to access uninitialised LateInit");
		unsafe { self.data.assume_init_mut() }
	}
}
