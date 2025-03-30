use acpi::madt::Madt;
use super::Registers;

pub(super) struct X2Apic {}

impl X2Apic {
	pub(super) fn init(madt: &Madt) {
		todo!()
	}
}

impl InterruptController for X2Apic {
	fn route_line_to(&self, line: Line, vector: Vector) {
		todo!()
	}

	fn unmask_line(&self, line: Line) {
		todo!()
	}

	fn mask_line(&self, line: Line) {
		todo!()
	}

	fn eoi(&self, line: Line) {
		todo!()
	}
}

macro_rules! x2apic_msr {
	(Registers.icr_low) => { compile_error!("Cannot access ICR through high/low - use `icr` instead"); };
	(Registers.icr_high) => { compile_error!("Cannot access ICR through high/low - use `icr` instead"); };
	(Registers.self_ipi) => { 0x83F };
	(Registers.icr) => { 0x830 };
    (Registers.$field:ident) => {
	    0x800 + ::core::mem::offset_of!(Registers, $field) / 16
    };
}
