#[allow(unused_imports)] use crate::prelude::*;
use acpi::madt::MadtEntry;
use crate::hal::timing::Timer3;
use crate::hal;
//use crate::hal::arch::apic::ioapic::{ActiveLevel, TriggerMode};
use kernel_api::time::Instant;

//mod ioapic;
pub(in crate::hal) mod lapic;

pub(in crate::hal) fn init() {
	let Ok(madt) = hal::acpi::tables().find_table::<::acpi::madt::Madt>() else {
		panic!("No MADT found");
	};
	for entry in madt.entries() {
		match entry {
			/*MadtEntry::IoApic(ioapic) => {
				ioapic::Ioapic::init(ioapic);
			}
			MadtEntry::InterruptSourceOverride(iso) => {
				if iso.bus != 0 { warn!("Unknown interrupt bus: {iso:?}"); }
				else {
					let level = if iso.flags & 2 == 0 { ActiveLevel::High } else { ActiveLevel::Low };
					let mode = if iso.flags & 8 == 0 { TriggerMode::Edge } else { TriggerMode::Level };
					let entry = (iso.global_system_interrupt, mode, level);
					let legacy_gsi_mapping = ioapics.legacy_map();
					match iso.irq {
						0 => legacy_gsi_mapping.pit = entry,
						1 => legacy_gsi_mapping.ps2_keyboard = entry,
						3 => legacy_gsi_mapping.com2 = entry,
						4 => legacy_gsi_mapping.com1 = entry,
						5 => legacy_gsi_mapping.lpt2 = entry,
						6 => legacy_gsi_mapping.floppy = entry,
						8 => legacy_gsi_mapping.rtc = entry,
						12 => legacy_gsi_mapping.ps2_mouse = entry,
						14 => legacy_gsi_mapping.ata_primary = entry,
						15 => legacy_gsi_mapping.ata_secondary = entry,
						irq => warn!("Unused GSI override: {irq}={entry:?}"),
					}
				}
			}*/
			_ => {}
		}
	}

	{
		macro_rules! ioapic_legacy_setup {
            ($ioapics:ident.$entry:ident) => {
	            let entry_meta = ioapics.legacy_map(). $entry;
				if let Some(mut redirection_entry) = $ioapics .redirection_entry(entry_meta.0) {
					redirection_entry.update(|entry| {
						entry.trigger_mode = entry_meta.1;
						entry.polarity = entry_meta.2;
						entry.mask = true;
					});
				}
            };
		}
		
		/*ioapic_legacy_setup!(ioapics.pit);
		ioapic_legacy_setup!(ioapics.ps2_keyboard);
		ioapic_legacy_setup!(ioapics.com2);
		ioapic_legacy_setup!(ioapics.com1);
		ioapic_legacy_setup!(ioapics.lpt2);
		ioapic_legacy_setup!(ioapics.floppy);
		ioapic_legacy_setup!(ioapics.rtc);
		ioapic_legacy_setup!(ioapics.ps2_mouse);
		ioapic_legacy_setup!(ioapics.ata_primary);
		ioapic_legacy_setup!(ioapics.ata_secondary);*/
	}

	lapic::init_lapic(&*madt);

	/*let mut timer_lvt = apic_boxed.project::<Apic::timer_lvt>();
	apic_boxed.project::<Apic::timer_initial_count>().write(0);
	let val = timer_lvt.read()
		.with_mask(false);
	timer_lvt.write(val);

	LAPIC.0.get_or_init(|| unsafe { Syncify::new(IrqCell::new(apic)) });*/
	//IOAPICS.get_or_init(|| unsafe { Spinlock::new(Syncify::new(ioapics)) });
}

pub fn send_self_ipi(vector: usize) {
	assert!(48 <= vector && vector < 256, "Invalid IPI vector");
	#[cfg(feature = "log.scheduler")] debug!("self IPI vector {vector:#x}");
	let vector = vector as u32;
	
	todo!()
	
	/*let lapic = LAPIC.0.get().expect("APIC not initialised");
	let lapic = lapic.lock();
	let lapic = unsafe { MmioCell::new(lapic.virtual_start().as_ptr()) };
	let mut icr = lapic.project::<Apic::icr_low>();
	while icr.read() & (1 << 12) != 0 { core::hint::spin_loop(); } // wait for any previous pending IPIs to send
	icr.write(vector | (1 << 14) | (0b01 << 18));
	while icr.read() & (1 << 12) != 0 { core::hint::spin_loop(); } // wait to send*/
}
