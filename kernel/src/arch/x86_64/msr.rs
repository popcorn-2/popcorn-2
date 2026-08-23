use core::arch::asm;

#[derive(Debug, Copy, Clone)]
pub struct ModelSpecificRegister(usize);

pub const IA32_APIC_BASE: ModelSpecificRegister = ModelSpecificRegister(0x1B);
pub const IA32_TSC_DEADLINE: ModelSpecificRegister = ModelSpecificRegister(0x6e0);
pub const STAR: ModelSpecificRegister = ModelSpecificRegister(0xC000_0081);
pub const LSTAR: ModelSpecificRegister = ModelSpecificRegister(0xC000_0082);
pub const SFMASK: ModelSpecificRegister = ModelSpecificRegister(0xC000_0084);
pub const FS_BASE: ModelSpecificRegister = ModelSpecificRegister(0xC000_0100);
pub const GS_BASE: ModelSpecificRegister = ModelSpecificRegister(0xC000_0101);
pub const KERNEL_GS_BASE: ModelSpecificRegister = ModelSpecificRegister(0xC000_0102);

impl ModelSpecificRegister {
	pub fn read(self) -> u64 {
		let (low, high): (u32, u32);

		unsafe {
			asm!(
				"rdmsr",
				in("rcx") self.0,
				out("eax") low,
				out("edx") high,
				options(nostack, nomem, preserves_flags)
			);
		}

		u64::from(low) | u64::from(high) << 32
	}

	pub fn write(self, value: u64) {
		let low = value.truncate::<u32>();
		let high = (value >> 32).truncate::<u32>();

		unsafe {
			asm!(
				"wrmsr",
				in("rcx") self.0,
				in("eax") low,
				in("edx") high,
				options(nostack, nomem, preserves_flags)
			);
		}
	}
}
