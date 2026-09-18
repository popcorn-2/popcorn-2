use core::arch::{asm, global_asm};
use crate::arch::x86_64::gdt::Gdt;
use crate::arch::x86_64::msr;
use crate::percpu::Percpu;
use core::mem::offset_of;
use core::sync::atomic::Ordering;
use crate::{percpu, syscall};
use crate::task::Task;

global_asm!(
	include_str!("asm/syscall_stub.asm"),
	entrypoint = sym entry,
	kernel_scratch_offset = const offset_of!(Percpu, arch.tss.0.scratch),
	rsp0_offset = const offset_of!(Percpu, arch.tss.0.rsp0),
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
	tss_scratch: u64,
	rflags: u64,
	fs_base: u64,
	gs_base: u64,
}

impl StackFrame {
	pub fn from(task: &Task) -> Self {
		Self {
			rip: task.registers.rip.load(Ordering::Relaxed),
			tss_scratch: task.registers.tss_scratch.load(Ordering::Relaxed),
			rflags: task.registers.rflags.load(Ordering::Relaxed),
			fs_base: task.registers.fs_base.load(Ordering::Relaxed),
			gs_base: task.registers.gs_base.load(Ordering::Relaxed),
		}
	}

	pub fn store(&self, task: &Task) {
		task.registers.rip.store(self.rip, Ordering::Relaxed);
		task.registers.tss_scratch.store(self.tss_scratch, Ordering::Relaxed);
		task.registers.rflags.store(self.rflags, Ordering::Relaxed);
		task.registers.fs_base.store(self.fs_base, Ordering::Relaxed);
		task.registers.gs_base.store(self.gs_base, Ordering::Relaxed);
	}
}

extern "rust-preserve-none" fn entry(
	r12: u64,
	user_gsbase: u64,
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
	let fs_base;
	unsafe { asm!("rdfsbase {:r}", out(reg) fs_base, options(nomem, nostack, preserves_flags)); }
	let stack_frame = StackFrame {
		rip,
		tss_scratch: percpu!(arch).tss.get_scratch(),
		rflags,
		fs_base,
		gs_base: user_gsbase,
	};

	let params = syscall::EntryParams {
		handle_num: rax.truncate::<u32>(),
		method: (rax >> 32).truncate::<u16>(),
		interface: r12,
		integer_args: [rdi, rsi, rdx],
		oob_args: [r8, r9],
		stack_frame,
	};

	// SAFETY: `entry` is only called from running thread
	let result = unsafe {
		syscall::entry(params)
	};

	// fixme: we probably leak kernel data in registers on return
	match result {
		Ok(exit_params) => {
			percpu!(arch).tss.set_scratch(exit_params.stack_frame.tss_scratch);
			unsafe { asm!("wrfsbase {:r}", in(reg) exit_params.stack_frame.fs_base, options(nomem, nostack, preserves_flags)); }

			let rax = exit_params.oid.widen::<u64>() | (exit_params.method.widen::<u64>() << 32);

			// this is kinda questionable since the compiler could put stuff after this
			unsafe {
				asm!(
					"",
					in("r12") exit_params.interface,
					in("r13") exit_params.stack_frame.gs_base,
					in("rdi") exit_params.integer_args[0],
					in("rsi") exit_params.integer_args[1],
					in("r8") exit_params.oob_args[0],
					in("r9") exit_params.oob_args[1],
					in("rcx") exit_params.stack_frame.rip,
					in("r11") exit_params.stack_frame.rflags,
				);
			}

			(rax, exit_params.integer_args[2])
		},
		Err(e) => {
			// this is kinda questionable since the compiler could put stuff after this
			unsafe {
				asm!(
					"",
					in("r13") stack_frame.gs_base,
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
