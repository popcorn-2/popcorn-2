use core::ops::{Deref, DerefMut};

pub enum LateInit<T> {
	Uninit,
	Init(T)
}

impl<T> LateInit<T> {
	pub const fn new() -> Self {
		Self::Uninit
	}

	pub fn init(&mut self, val: T) -> &mut T {
		*self = Self::Init(val);
		self
	}
}

impl<T> Deref for LateInit<T> {
	type Target = T;

	#[track_caller]
	fn deref(&self) -> &Self::Target {
		match self {
			Self::Uninit => panic!("Tried to access uninitialised LateInit"),
			Self::Init(inner) => inner
		}
	}
}

impl<T> DerefMut for LateInit<T> {
	#[track_caller]
	fn deref_mut(&mut self) -> &mut Self::Target {
		match self {
			Self::Uninit => panic!("Tried to access uninitialised LateInit"),
			Self::Init(inner) => inner
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
		init.init(5);
		assert_eq!(*init, 5);
	}
}
