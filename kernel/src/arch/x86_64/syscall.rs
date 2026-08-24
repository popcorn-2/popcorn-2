use core::arch::global_asm;
use crate::arch::x86_64::gdt::Gdt;
use crate::arch::x86_64::msr;
use crate::percpu::Percpu;
use core::mem::offset_of;

#[cfg(not(feature = "syscall-abi-next"))]
global_asm!(
	include_str!("asm/syscall_stub.asm"),
	entrypoint = sym crate::syscall_handler,
	rsp0_offset = const offset_of!(Percpu, kernel_stack_top),
);

#[cfg(feature = "syscall-abi-next")]
global_asm!(
	include_str!("asm/syscall_stub_next.asm"),
	entrypoint = sym crate::syscall::entry,
	kernel_scratch_offset = const offset_of!(Percpu, arch.tss.scratch),
	rsp0_offset = const offset_of!(Percpu, kernel_stack_top),
);

unsafe extern "custom" {
	fn x86_64_syscall_stub();
}

pub fn init() {
	const {
		assert!(Gdt::KERNEL_CODE_SEGMENT + 8 == Gdt::KERNEL_DATA_SEGMENT, "incorrect GDT layout for fast syscall");
		assert!(Gdt::USER_DATA_SEGMENT + 8 == Gdt::USER_CODE_SEGMENT, "incorrect GDT layout for fast syscall");
	}

	msr::LSTAR.write((x86_64_syscall_stub as *const ()).addr() as u64);
	msr::SFMASK.write(
		1 << 10 | // direction flag
		1 << 9 // interrupt flag
	);
	msr::STAR.write(
		(Gdt::KERNEL_CODE_SEGMENT as u64) << 32 |
			(Gdt::USER_DATA_SEGMENT as u64 - 8) << 48
	);
}
