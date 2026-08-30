use acpi::{AcpiError, AcpiTables};
use acpi::sdt::fadt::Fadt;
use crate::arch::acpi::gas::read_gas;

const SCI_EN: u64 = acpi::registers::Pm1ControlBit::SciEnable as u64;

pub fn init_fadt<H: acpi::Handler>(tables: &AcpiTables<H>) -> Result<(), AcpiError> {
	let Some(fadt) = tables.find_table::<Fadt>() else {
		panic!("No FADT found");
	};

	#[cfg(target_arch = "x86_64")]
	if fadt.smi_cmd_port != 0 {
		debug!("enabling SCI");
		unsafe {
			core::arch::asm!("out dx, al", in("dx") fadt.smi_cmd_port, in("al") fadt.acpi_enable, options(nostack, preserves_flags));
		}

		let pm1a_control = fadt.pm1a_control_block()?;
		let pm1b_control = fadt.pm1b_control_block()?;

		loop {
			let mut pm1_val = read_gas(pm1a_control)?;
			if let Some(pm1b_control) = pm1b_control {
				pm1_val |= read_gas(pm1b_control)?;
			}

			if pm1_val & SCI_EN {
				debug!("SCI enabled");
				break;
			}
		}
	}

	Ok(())
}
