use core::arch::asm;
use core::marker::PhantomData;

pub trait PortWidth {}
impl PortWidth for u8 {}
impl PortWidth for u16 {}
impl PortWidth for u32 {}

#[derive(Debug)]
pub struct Port<T>(u16, PhantomData<T>);

impl<T> Port<T> where T: PortWidth {
	pub const fn new(addr: u16) -> Self {
		Self(addr, PhantomData)
	}
}

impl Port<u8> {
	#[inline(always)]
	pub unsafe fn read(&self) -> u8 {
		let ret;
		unsafe { asm!("in al, dx", in("dx") self.0, out("al") ret, options(nostack, preserves_flags)); }
		ret
	}

	#[inline(always)]
	pub unsafe fn write(&mut self, val: u8) {
		unsafe { asm!("out dx, al", in("dx") self.0, in("al") val, options(nostack, preserves_flags)); }
	}
}

impl Port<u16> {
	#[inline(always)]
	pub unsafe fn read(&self) -> u16 {
		let ret;
		unsafe { asm!("in ax, dx", in("dx") self.0, out("ax") ret, options(nostack, preserves_flags)); }
		ret
	}

	#[inline(always)]
	pub unsafe fn write(&mut self, val: u16) {
		unsafe { asm!("out dx, ax", in("dx") self.0, in("ax") val, options(nostack, preserves_flags)); }
	}
}

impl Port<u32> {
	#[inline(always)]
	pub unsafe fn read(&self) -> u32 {
		let ret;
		unsafe { asm!("in eax, dx", in("dx") self.0, out("eax") ret, options(nostack, preserves_flags)); }
		ret
	}

	#[inline(always)]
	pub unsafe fn write(&mut self, val: u32) {
		unsafe { asm!("out dx, eax", in("dx") self.0, in("eax") val, options(nostack, preserves_flags)); }
	}
}
