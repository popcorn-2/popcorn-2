pub(crate) unsafe trait Field {
	type Base;
	type Inner;
	const OFFSET: usize;
}

use core::mem::MaybeUninit;
use core::pin::Pin;
pub use macros::Fields as Field;

pub trait ProjectSuper<T> {
	type Projected<'a, A: 'a>: Project<A>;
}

pub trait Project<T>: ProjectSuper<T> {
	fn project<'a, F: Field<Base = T>>(self) -> Self::Projected<'a, F::Inner> where Self: 'a, F::Inner: 'a;
}

impl<T> ProjectSuper<T> for Pin<&mut T> {
	type Projected<'a, A: 'a> = Pin<&'a mut A>;
}

impl<T> Project<T> for Pin<&mut T> {
	fn project<'a, F: Field<Base=T>>(self) -> Pin<&'a mut F::Inner> where Self: 'a {
		unsafe {
			self.map_unchecked_mut(|s| &mut *(s as *mut T).byte_add(F::OFFSET).cast())
		}
	}
}

impl<T> ProjectSuper<T> for Pin<&T> {
	type Projected<'a, A: 'a> = Pin<&'a A>;
}

impl<T> Project<T> for Pin<&T> {
	fn project<'a, F: Field<Base=T>>(self) -> Pin<&'a F::Inner> where Self: 'a {
		unsafe {
			self.map_unchecked(|s| &*(s as *const T).byte_add(F::OFFSET).cast())
		}
	}
}

impl<T> ProjectSuper<T> for &mut MaybeUninit<T> {
	type Projected<'a, A: 'a> = &'a mut MaybeUninit<A>;
}

impl<T> Project<T> for &mut MaybeUninit<T> {
	fn project<'a, F: Field<Base=T>>(self) -> &'a mut MaybeUninit<F::Inner> where Self: 'a {
		unsafe {
			&mut *self.as_mut_ptr().byte_add(F::OFFSET).cast()
		}
	}
}

impl<T> ProjectSuper<T> for &MaybeUninit<T> {
	type Projected<'a, A: 'a> = &'a MaybeUninit<A>;
}

impl<T> Project<T> for &MaybeUninit<T> {
	fn project<'a, F: Field<Base=T>>(self) -> &'a MaybeUninit<F::Inner> where Self: 'a {
		unsafe {
			&*self.as_ptr().byte_add(F::OFFSET).cast()
		}
	}
}
