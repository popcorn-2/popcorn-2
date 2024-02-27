pub enum Exception {
	FloatingPoint,
	Debug(DebugTy),
	IllegalInstruction,
	PageFault(PageFault),
	Generic(&'static str),
	BusFault,
	Nmi,
	Unknown(&'static str),
}

pub enum DebugTy {
	Breakpoint
}

pub struct PageFault {

}
