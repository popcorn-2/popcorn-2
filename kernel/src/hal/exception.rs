use core::fmt::{Debug, Display, Formatter};
use kernel_api::memory::VirtualAddress;

pub struct Exception<'a> {
	pub ty: Ty<'a>,
	pub registers: &'a mut dyn ExceptionRegisters,
	pub user_mode: bool,
}

pub trait ExceptionRegisters: Debug {
	fn ip(&self) -> usize;
	fn set_ip(&mut self, val: usize);
	#[expect(dead_code)]
	fn set_return_reg(&mut self, val: usize);
}

#[derive(Debug)]
pub enum Ty<'a> {
	FloatingPoint,
	Debug(DebugTy),
	IllegalInstruction,
	PageFault(PageFault<'a>),
	BusFault,
	Nmi,
	Panic,
	Generic(&'static str),
	Unknown(&'static str),
}

/*
impl PartialEq for Ty<'_> {
	fn eq(&self, other: &Self) -> bool {
		match (&self, other) {
			(&Ty::FloatingPoint, &Ty::FloatingPoint) => true,
			(&Ty::Debug(a), &Ty::Debug(ref b)) => a == b,
			(&Ty::IllegalInstruction, &Ty::IllegalInstruction) => true,
			(&Ty::PageFault(a), &Ty::PageFault(ref b)) => a. == b,
			(&Ty::BusFault, &Ty::BusFault) => true,
			(&Ty::Nmi, &Ty::Nmi) => true,
			(&Ty::Panic, &Ty::Panic) => true,
			(&Ty::Generic(_), &Ty::Generic(_)) => true,
			(&Ty::Unknown(_), &Ty::Unknown(_)) => true,
			_ => false,
		}
	}
}

impl Eq for Ty<'_> {}*/

#[derive(Debug, Eq, PartialEq)]
pub enum DebugTy {
	Breakpoint
}

#[derive(Debug)]
pub struct PageFault<'a> {
	pub access_addr: VirtualAddress,
	pub meta: PageFaultMeta<'a>,
}

#[derive(Debug, Copy, Clone)]
pub struct PageFaultMeta<'a> {
	pub meta: usize,
	pub arch_meta: &'a dyn Debug,
}

impl PageFaultMeta<'_> {
	pub fn present(self) -> bool { self.meta & 1 != 0 }
}

impl Display for PageFault<'_> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "Attempted to access address {:#x}: {:#b}\n\t{:?}", self.access_addr, self.meta.meta, self.meta.arch_meta)
	}
}
