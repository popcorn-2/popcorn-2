use core::arch::asm;

pub fn get_and_disable_interrupts() -> usize {
	let flags: usize;
	unsafe {
		asm!(
			"pushfq",
			"pop {}",
			"cli",
			out(reg) flags, options(preserves_flags),
		)
	}

	flags & 0x0200
}
