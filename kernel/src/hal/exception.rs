use core::fmt::{Debug, Display, Formatter};
use derive_more::Display;
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

#[derive(Display, Debug)]
pub enum Ty<'a> {
	#[display(fmt = "Floating point exception")]
	FloatingPoint,
	#[display(fmt = "{_0}")]
	Debug(DebugTy),
	#[display(fmt = "Illegal instruction")]
	IllegalInstruction,
	#[display(fmt = "{_0}")]
	PageFault(PageFault<'a>),
	#[display(fmt = "Bus error")]
	BusFault,
	#[display(fmt = "NMI")]
	Nmi,
	#[display(fmt = "Kernel panic")]
	Panic,
	#[display(fmt = "Arch specific: {_0}")]
	Generic(&'static str),
	#[display(fmt = "Arch specific: {_0}")]
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

#[derive(Display, Debug, Eq, PartialEq)]
pub enum DebugTy {
	#[display(fmt = "Breakpoint hit")]
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
