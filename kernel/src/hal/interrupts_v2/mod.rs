/// An interrupt vector on a core
/// 
/// - RISC-V: `mcause` value
/// - amd64: IDT vector number
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Vector(pub usize);

/*
/// An external interrupt line
/// 
/// ACPI GSI
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Line(pub usize);

trait LocalInterruptController {
	
}

struct LocalInterruptControllerMeta {
	
}
*/
