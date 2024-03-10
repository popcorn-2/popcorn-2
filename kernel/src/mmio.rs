use core::fmt::{Formatter, Pointer};
use crate::projection::{Field, Project, ProjectSuper};

#[repr(transparent)]
pub struct MmioCell<T> {
	ptr: *mut T
}

impl<T> Clone for MmioCell<T> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<T> Copy for MmioCell<T> {}

impl<T> Pointer for MmioCell<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		self.ptr.fmt(f)
	}
}

impl<T> MmioCell<T> {
	pub unsafe fn new(ptr: *mut T) -> Self {
		Self { ptr }
	}
}

impl<T: Copy> MmioCell<T> {
	#[must_use]
	pub fn read(&self) -> T {
		unsafe { self.ptr.read_volatile() }
	}

	pub fn write(&mut self, val: T) {
		unsafe { self.ptr.write_volatile(val) }
	}
}

impl<T> ProjectSuper<T> for MmioCell<T> {
	type Projected<'a, A: 'a> = MmioCell<A>;
}

impl<T> Project<T> for MmioCell<T> {
	fn project<'a, F: Field<Base = T>>(self) -> MmioCell<F::Inner> where Self: 'a {
		unsafe {
			MmioCell::new(self.ptr.byte_add(F::OFFSET).cast())
		}
	}
}
