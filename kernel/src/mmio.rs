use core::fmt::{Formatter, Pointer};
use crate::projection::{Field, Project, ProjectSuper};

#[repr(transparent)]
pub struct MmioBox<T> {
	ptr: *mut T
}

impl<T> Clone for MmioBox<T> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<T> Copy for MmioBox<T> {}

impl<T> Pointer for MmioBox<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		self.ptr.fmt(f)
	}
}

impl<T> MmioBox<T> {
	pub unsafe fn new(ptr: *mut T) -> Self {
		Self { ptr }
	}
}

impl<T: Copy> MmioBox<T> {
	#[must_use]
	pub fn read(&self) -> T {
		unsafe { self.ptr.read_volatile() }
	}

	pub fn write(&mut self, val: T) {
		unsafe { self.ptr.write_volatile(val) }
	}
}

impl<T> ProjectSuper<T> for MmioBox<T> {
	type Projected<'a, A: 'a> = MmioBox<A>;
}

impl<T> Project<T> for MmioBox<T> {
	fn project<'a, F: Field<Base = T>>(self) -> MmioBox<F::Inner> where Self: 'a {
		unsafe {
			MmioBox::new(self.ptr.byte_add(F::OFFSET).cast())
		}
	}
}
