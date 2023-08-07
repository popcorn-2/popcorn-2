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
