use core::fmt::Debug;
use core::marker::PhantomData;
use core::ptr::{addr_of, addr_of_mut};
use log::warn;

mod private {
	use core::mem;

	pub trait Sealed {}

	trait IsTrue<const VAL: bool> {}
	impl IsTrue<true> for () {}

	pub trait U32Sized {}
	impl<T> U32Sized for T where (): IsTrue<{mem::size_of::<u32>() == mem::size_of::<T>()}> {}
}

pub trait Mode: private::Sealed {}
pub enum Allow {}
pub enum Deny {}
pub enum Warn {}

impl private::Sealed for Allow {}
impl private::Sealed for Deny {}
impl private::Sealed for Warn {}

impl Mode for Allow {}
impl Mode for Deny {}
impl Mode for Warn {}

#[repr(C, align(16))]
pub struct Register<R: Mode = Deny, W: Mode = Deny, T = u32>(T, PhantomData<(R, W)>) where T: private::U32Sized;

impl<R: Mode, T> Register<R, Allow, T> where T: private::U32Sized {
	pub unsafe fn write_register(self: *mut Self, val: T) {
		addr_of_mut!((*self).0).write_volatile(val)
	}
}

impl<R: Mode, T: Debug> Register<R, Warn, T> where T: private::U32Sized {
	pub unsafe fn write_register(self: *mut Self, val: T) {
		warn!("Writing to `warn` register at {self:p} with value {val:?}");
		addr_of_mut!((*self).0).write_volatile(val)
	}
}

impl<W: Mode, T> Register<Allow, W, T> where T: private::U32Sized {
	pub unsafe fn read_register(self: *mut Self) -> T {
		addr_of!((*self).0).read_volatile()
	}
}

impl<W: Mode, T> Register<Warn, W, T> where T: private::U32Sized {
	pub unsafe fn read_register(self: *mut Self) -> T {
		warn!("Reading from `warn` register at {self:p}");
		addr_of!((*self).0).read_volatile()
	}
}
