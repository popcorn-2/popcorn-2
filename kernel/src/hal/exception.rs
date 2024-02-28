use core::fmt::{Display, Formatter};
use derive_more::Display;

pub struct Exception {
	pub ty: Ty,
	pub at_instruction: usize,
}

#[derive(Display, Debug, Eq, PartialEq)]
pub enum Ty {
	#[display(fmt = "Floating point exception")]
	FloatingPoint,
	#[display(fmt = "{_0}")]
	Debug(DebugTy),
	#[display(fmt = "Illegal instruction")]
	IllegalInstruction,
	#[display(fmt = "{_0}")]
	PageFault(PageFault),
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

#[derive(Display, Debug, Eq, PartialEq)]
pub enum DebugTy {
	#[display(fmt = "Breakpoint hit")]
	Breakpoint
}

#[derive(Debug, Eq, PartialEq)]
pub struct PageFault {
	pub access_addr: usize
}

impl Display for PageFault {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "Attempted to access address {:#x}", self.access_addr)
	}
}
