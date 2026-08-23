mod handler;
mod idt;

use core::arch::{asm, global_asm};
use core::panic::AssertUnwindSafe;
use kernel_api::memory::VirtualAddress;
use crate::hal::exception::{DebugTy, Exception, PageFault, PageFaultMeta, Ty};
use crate::arch::x86_64::msr;

pub use idt::IDT;

global_asm!(
	include_str!("../asm/interrupt_stub.asm"),
	entrypoint = sym x86_64_interrupt_entry,
	code_segment = const super::gdt::Gdt::KERNEL_CODE_SEGMENT,
);

unsafe extern "custom" {
	fn x86_64_interrupt_stub();
}

#[repr(C)]
struct StackFrame {
	r11: u64,
	r10: u64,
	r9: u64,
	r8: u64,
	rcx: u64,
	rdx: u64,
	rsi: u64,
	rdi: u64,
	rax: u64,
	num: u64,
	error: u64,
	rip: u64,
	cs: u64,
	flags: u64,
	rsp: u64,
	ss: u64,
}

extern "C" fn x86_64_interrupt_entry(data: &mut StackFrame) {
	let data = AssertUnwindSafe(data);
	match crate::panicking::catch_unwind(move || {
		info!("[amd64] vector {:#x}", data.num);
		todo!()
	}) {
		Ok(_) => {},
		Err(_) => {
			error!("panic in interrupt handler");
			loop {}
		}
	}
}

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
