use crate::hal;
//use crate::hal::arch::apic::ioapic::{ActiveLevel, TriggerMode};

//mod ioapic;
pub(in crate::hal) mod lapic;

pub(in crate::hal) fn init() {
	let Some(madt) = hal::acpi::tables().find_table::<::acpi::sdt::madt::Madt>() else {
		panic!("No MADT found");
	};

	lapic::init_lapic(madt.get());
}

#[expect(dead_code)]
pub fn send_self_ipi(vector: usize) {
	assert!(48 <= vector && vector < 256, "Invalid IPI vector");
	// let vector = vector as u32;
	
	todo!()
	
	/*let lapic = LAPIC.0.get().expect("APIC not initialised");
	let lapic = lapic.lock();
	let lapic = unsafe { MmioCell::new(lapic.virtual_start().as_ptr()) };
	let mut icr = lapic.project::<Apic::icr_low>();
	while icr.read() & (1 << 12) != 0 { core::hint::spin_loop(); } // wait for any previous pending IPIs to send
	icr.write(vector | (1 << 14) | (0b01 << 18));
	while icr.read() & (1 << 12) != 0 { core::hint::spin_loop(); } // wait to send*/
}
