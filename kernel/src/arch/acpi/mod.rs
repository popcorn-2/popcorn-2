use acpi::sdt::fadt::Fadt;
use acpi::sdt::madt::Madt;
use crate::handoff::ParsedHandoff;

mod gas;
mod fadt;
mod memory;

pub fn post_memory_init(handoff: &ParsedHandoff) {
	debug!("initialising ACPI platform");
	let tables = unsafe {
		acpi::AcpiTables::from_rsdp(memory::handler(), handoff.rsdp.addr)
			.expect("malformed ACPI tables")
	};

	fadt::init_fadt(&tables).expect("ACPI error");
	info!("ACPI initialization done");
}
