mod handler;
mod idt;

use core::arch::{asm, global_asm};
use core::panic::AssertUnwindSafe;
use core::mem::offset_of;
use core::sync::atomic::Ordering;
use crate::percpu::Percpu;

pub use idt::IDT;
use kernel_api::ptr::LocalUser;
use crate::percpu;

global_asm!(
	include_str!("../asm/interrupt_stub.asm"),
	entrypoint = sym x86_64_interrupt_entry,
	needs_resched_offset = const offset_of!(Percpu, needs_reschedule),
	code_segment = const super::gdt::Gdt::KERNEL_CODE_SEGMENT,
	scheduler_entry = sym crate::task::scheduler_entry,
);

unsafe extern "custom" {
	fn x86_64_interrupt_stub();
}

#[derive(Debug)]
#[repr(C)]
pub struct StackFrame {
	r15: u64,
	r14: u64,
	r13: u64,
	r12: u64,
	r11: u64,
	r10: u64,
	r9: u64,
	r8: u64,
	rbp: u64,
	rdi: u64,
	rsi: u64,
	rdx: u64,
	rcx: u64,
	rbx: u64,
	rax: u64,
	num: u64,
	error: u64,
	rip: u64,
	cs: u64,
	flags: u64,
	rsp: u64,
	ss: u64,
}

impl StackFrame {
	pub fn store_to(&self, to: &super::SavedRegisters) {
		to.rax.store(self.rax, Ordering::Relaxed);
		to.rbx.store(self.rbx, Ordering::Relaxed);
		to.rcx.store(self.rcx, Ordering::Relaxed);
		to.rdx.store(self.rdx, Ordering::Relaxed);
		to.rsi.store(self.rsi, Ordering::Relaxed);
		to.rdi.store(self.rdi, Ordering::Relaxed);
		to.rsp.store(self.rsp, Ordering::Relaxed);
		to.rbp.store(self.rbp, Ordering::Relaxed);
		to.r8.store(self.r8, Ordering::Relaxed);
		to.r9.store(self.r9, Ordering::Relaxed);
		to.r10.store(self.r10, Ordering::Relaxed);
		to.r11.store(self.r11, Ordering::Relaxed);
		to.r12.store(self.r12, Ordering::Relaxed);
		to.r13.store(self.r13, Ordering::Relaxed);
		to.r14.store(self.r14, Ordering::Relaxed);
		to.r15.store(self.r15, Ordering::Relaxed);
		to.rip.store(self.rip, Ordering::Relaxed);
		to.rflags.store(self.flags, Ordering::Relaxed);

		let fsbase;
		let gsbase;
		unsafe {
			asm!("rdfsbase {:r}", out(reg) fsbase, options(nomem, nostack, preserves_flags));
			asm!(
				// FIXME: interrupts
				"cli",
				"swapgs",
				"rdgsbase {:r}",
				"swapgs",
				out(reg) gsbase,
				options(nomem, nostack, preserves_flags),
			);
		}

		to.fs_base.store(fsbase, Ordering::Relaxed);
		to.gs_base.store(gsbase, Ordering::Relaxed);
	}

	pub fn load_from(&mut self, from: &super::SavedRegisters) {
		self.rax = from.rax.load(Ordering::Relaxed);
		self.rbx = from.rbx.load(Ordering::Relaxed);
		self.rcx = from.rcx.load(Ordering::Relaxed);
		self.rdx = from.rdx.load(Ordering::Relaxed);
		self.rsi = from.rsi.load(Ordering::Relaxed);
		self.rdi = from.rdi.load(Ordering::Relaxed);
		self.rsp = from.rsp.load(Ordering::Relaxed);
		self.rbp = from.rbp.load(Ordering::Relaxed);
		self.r8 = from.r8.load(Ordering::Relaxed);
		self.r9 = from.r9.load(Ordering::Relaxed);
		self.r10 = from.r10.load(Ordering::Relaxed);
		self.r11 = from.r11.load(Ordering::Relaxed);
		self.r12 = from.r12.load(Ordering::Relaxed);
		self.r13 = from.r13.load(Ordering::Relaxed);
		self.r14 = from.r14.load(Ordering::Relaxed);
		self.r15 = from.r15.load(Ordering::Relaxed);
		self.rip = from.rip.load(Ordering::Relaxed);
		self.flags = from.rflags.load(Ordering::Relaxed);

		let fsbase = from.fs_base.load(Ordering::Relaxed);
		let gsbase = from.gs_base.load(Ordering::Relaxed);

		unsafe {
			asm!("wrfsbase {:r}", in(reg) fsbase, options(nomem, nostack, preserves_flags));
			asm!(
				// FIXME: interrupts
				"cli",
				"swapgs",
				"wrgsbase {:r}",
				"swapgs",
				in(reg) gsbase,
				options(nomem, nostack, preserves_flags),
			);
		}
	}
}

extern "rust-preserve-none" fn x86_64_interrupt_entry(data: &mut StackFrame) {
	let data = AssertUnwindSafe(data);
	match crate::panicking::catch_unwind(move || {
		info!("[amd64] vector {:#x?}", data);
		if data.num == 0x06 {
			let ptr = unsafe { LocalUser::<*const u8>::new(data.rip as usize) };
			if let Ok(0x0f) = ptr.read()
				&& let Ok(0x0b) = unsafe { ptr.add(1) }.read()
				&& let Ok(27) = unsafe { ptr.add(2) }.read()
			{
				// trap with string
				let mut ptr = unsafe { ptr.add(3) };
				error!("userspace trap:");
				for _ in 0..256 {
					let Ok(c) = ptr.read() else { break; };
					if c == 0 { break; };
					let c = c as char;
					sprint!("{c}");
					unsafe { ptr = ptr.add(1) };
				}
				sprintln!();
			}
		}
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
