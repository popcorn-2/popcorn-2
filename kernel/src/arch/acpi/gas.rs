use acpi::AcpiError;
use acpi::address::{AddressSpace, GenericAddress, StandardAccessSize};

pub fn read_gas(gas: GenericAddress) -> Result<u64, AcpiError> {
	let access_width = match gas.standard_access_size()? {
		StandardAccessSize::Undefined => ((gas.bit_width + gas.bit_offset + 7) / 8).next_power_of_two(),
		StandardAccessSize::ByteAccess => 1,
		StandardAccessSize::WordAccess => 2,
		StandardAccessSize::DWordAccess => 4,
		StandardAccessSize::QWordAccess => 8,
	};

	let val = match gas.address_space {
		AddressSpace::SystemIo => {
			let val: u64;
			match access_width {
				1 => unsafe { core::arch::asm!("in al, dx", in("dx") gas.address, inout("rax") 0u64 => val, options(nostack, nomem, preserves_flags)) },
				2 => unsafe { core::arch::asm!("in ax, dx", in("dx") gas.address, inout("rax") 0u64 => val, options(nostack, nomem, preserves_flags)) },
				4 => unsafe { core::arch::asm!("in eax, dx", in("dx") gas.address, inout("rax") 0u64 => val, options(nostack, nomem, preserves_flags)) },
				_ => unimplemented!("unsupported access size for system IO")
			}
			val
		},
		address_space => unimplemented!("unsupported GAS type {:?}", address_space),
	};

	let mask = 1u64.checked_shl(gas.bit_width.into()).map(|val| val - 1).unwrap_or(u64::MAX);
	Ok((val >> gas.bit_offset) & mask)
}
