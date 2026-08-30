use acpi::sdt::fadt::Fadt;
use acpi::sdt::madt::Madt;
use kernel_api::memory::PhysicalAddress;
use crate::handoff::ParsedHandoff;

mod gas;
mod fadt;
mod memory;

pub fn post_memory_init(rsdp: PhysicalAddress) {
	debug!("initialising ACPI platform");
	let tables = unsafe {
		acpi::AcpiTables::from_rsdp(memory::handler(), rsdp.addr)
			.expect("malformed ACPI tables")
	};

	fadt::init_fadt(&tables).expect("ACPI error");
	info!("ACPI initialization done");
}
