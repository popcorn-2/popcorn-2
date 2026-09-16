use kernel_api::memory::PhysicalAddress;

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
