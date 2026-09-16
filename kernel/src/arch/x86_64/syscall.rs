use core::arch::{asm, global_asm};
use crate::arch::x86_64::gdt::Gdt;
use crate::arch::x86_64::msr;
use crate::percpu::Percpu;
use core::mem::offset_of;
use core::sync::atomic::Ordering;
use crate::arch::SavedRegisters;
use crate::task::Task;

global_asm!(
	include_str!("asm/syscall_stub_next.asm"),
	entrypoint = sym entry,
	kernel_scratch_offset = const offset_of!(Percpu, arch.tss.scratch),
	rsp0_offset = const offset_of!(Percpu, arch.tss.rsp0),
	needs_resched_offset = const offset_of!(Percpu, needs_reschedule),
	data_segment = const super::gdt::Gdt::USER_DATA_SEGMENT,
	code_segment = const super::gdt::Gdt::USER_CODE_SEGMENT,
);

unsafe extern "custom" {
	fn x86_64_syscall_stub();
}

#[derive(Copy, Clone, Debug)]
pub struct StackFrame {
	rip: u64,
	rflags: u64,
	tss_scratch: u64,
}

pub extern "rust-preserve-none" fn entry(
	r12: usize,
	r13: u64,
	_r14: u64,
	_r15: usize,
	rdi: usize,
	rsi: usize,
	rdx: usize,
	rip: u64,
	r8: usize,
	r9: usize,
	rflags: u64,
	rax: u64,
) -> (u64, usize) {
	let stack_frame = StackFrame {
		rip,
		rflags,
		tss_scratch: percpu_v2!(arch).tss.get_scratch(),
	};
	// SAFETY: `entry` is only called from running thread
	let result = unsafe {
		crate::syscall::entry(
			r12,
			rax.truncate::<u32>(),
			r13.truncate::<u16>(),
			(r13 >> 16).truncate::<u16>(),
			rdi,
			rsi,
			rdx,
			r9,
			r8,
			stack_frame,
		)
	};

	// fixme: we probably leak kernel data in registers on return
	match result {
		Ok((r12, rax, r13_lo, r13_hi, rdi, rsi, rdx, r9, r8, new_stack_frame)) => {
			percpu_v2!(arch).tss.set_scratch(new_stack_frame.tss_scratch);
			// this is kinda questionable since the compiler could put stuff after this
			unsafe {
				asm!(
					"",
					in("r12") r12,
					in("r13") r13_lo.widen::<u64>() | (r13_hi.widen::<u64>() << 16),
					in("rdi") rdi,
					in("rsi") rsi,
					in("r9") r9,
					in("r8") r8,
					in("rcx") new_stack_frame.rip,
					in("r11") new_stack_frame.rflags,
				);
			}
			(/* rax: */ rax.widen::<u64>(), /* rdx: */ rdx)
		},
		Err(e) => {
			percpu_v2!(arch).tss.set_scratch(stack_frame.tss_scratch);
			// this is kinda questionable since the compiler could put stuff after this
			unsafe {
				asm!(
					"",
					in("rcx") stack_frame.rip,
					in("r11") stack_frame.rflags,
				);
			}
			(/* rax: */ (-(e as i64)).cast_unsigned(), /* rdx: */ 0)
		}
	}
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
