use core::ops::{Deref, DerefMut};
use kernel_api::sync::OnceLock;

pub struct LateInit<T>(OnceLock<T>);

impl<T> LateInit<T> {
	pub const fn new() -> Self {
		Self(OnceLock::new())
	}

	pub fn init_ref(&self, val: T) -> &T {
		self.0.get_or_init(|| val);
		self.0.get().expect("Just initialised OnceLock")
	}

	pub fn init_mut(&mut self, val: T) -> &mut T {
		self.0.get_or_init(|| val);
		self.0.get_mut().expect("Just initialised OnceLock")
	}
}

impl<T> Deref for LateInit<T> {
	type Target = T;

	#[track_caller]
	fn deref(&self) -> &Self::Target {
		match self.0.get() {
			Some(inner) => inner,
			None => panic!("Tried to access uninitialised LateInit")
		}
	}
}

impl<T> DerefMut for LateInit<T> {
	#[track_caller]
	fn deref_mut(&mut self) -> &mut Self::Target {
		match self.0.get_mut() {
			Some(inner) => inner,
			None => panic!("Tried to access uninitialised LateInit")
		}
	}
}

#[cfg(test)]
mod tests {
	use core::hint::black_box;
	use super::*;

	#[test]
	#[should_panic = "Tried to access uninitialised LateInit"]
	fn panic_on_uninit_access() {
		let uninit = LateInit::<u8>::new();
		black_box(uninit.deref());
	}

	#[test]
	fn no_panic_once_init() {
		let mut init = LateInit::<u8>::new();
		init.init_mut(5);
		assert_eq!(*init, 5);
	}
}
