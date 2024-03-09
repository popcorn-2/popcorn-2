use crate::projection::{Field, Project, ProjectSuper};

#[repr(transparent)]
pub struct MmioBox<T> {
	ptr: *mut T
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

impl<T> ProjectSuper<T> for &MmioBox<T> {
	type Projected<'a, A: 'a> = &'a MmioBox<A>;
}

impl<T> Project<T> for &MmioBox<T> {
	fn project<'a, F: Field<Base = T>>(self) -> &'a MmioBox<F::Inner> where Self: 'a {
		unsafe {
			&*self.ptr.byte_add(F::OFFSET).cast()
		}
	}
}

impl<T> ProjectSuper<T> for &mut MmioBox<T> {
	type Projected<'a, A: 'a> = &'a mut MmioBox<A>;
}

impl<T> Project<T> for &mut MmioBox<T> {
	fn project<'a, F: Field<Base = T>>(self) -> &'a mut MmioBox<F::Inner> where Self: 'a {
		unsafe {
			&mut *self.ptr.byte_add(F::OFFSET).cast()
		}
	}
}
