use core::arch::asm;
use core::mem;
use core::mem::offset_of;
use log::warn;
use crate::{Hal, SaveState, ThreadControlBlock};
use crate::arch::amd64::idt::entry::Type;
use crate::arch::amd64::idt::handler::InterruptStackFrame;
use crate::arch::amd64::idt::Idt;

mod gdt;
mod tss;
mod idt;
mod serial;
mod port;
mod qemu;
mod paging2;
pub(crate) mod paging;

#[derive(Hal)]
struct Amd64Hal;

unsafe impl Hal for Amd64Hal {
	type SerialOut = serial::HalWriter;
	type KTableTy = paging2::Amd64KTable;
	type TTableTy = paging2::Amd64TTable;
	type SaveState = Amd64SaveState;

	fn breakpoint() { unsafe { asm!("int3"); } }

	fn exit(result: crate::Result) -> ! {
		qemu::debug_exit(result)
	}

	fn debug_output(data: &[u8]) -> Result<(), ()> {
		qemu::debug_con_write(data);
		Ok(())
	}

	fn early_init() {
		let tss = tss::TSS.get_or_init(|| {
			// TODO: actually load stacks
			tss::Tss::new()
		});

		let gdt = gdt::GDT.get_or_init(|| {
			use gdt::{Entry, EntryTy, Privilege};

			let mut gdt = gdt::Gdt::new();
			gdt.add_entry(EntryTy::KernelCode, Entry::new(Privilege::Ring0, true, true));
			gdt.add_entry(EntryTy::KernelData, Entry::new(Privilege::Ring0, false, true));
			gdt.add_entry(EntryTy::UserLongCode, Entry::new(Privilege::Ring3, true, true));
			gdt.add_entry(EntryTy::UserData, Entry::new(Privilege::Ring3, false, true));
			gdt.add_tss(tss);
			gdt
		});

		gdt.load();
		gdt.load_tss();
	}

	fn init_idt() {
		let idt = idt::IDT.get_or_init(|| {
			let mut idt = Idt::new();
			idt.breakpoint = idt::entry::Entry::new(breakpoint, None, 3, Type::InterruptGate);
			idt
		});
		idt.load();
	}

	#[export_name = "__popcorn_enable_irq"]
	fn enable_interrupts() {
		unsafe { asm!("sti", options(preserves_flags, nomem)); }
	}

	#[export_name = "__popcorn_disable_irq"]
	fn get_and_disable_interrupts() -> usize {
		let flags: usize;
		unsafe {
			asm!("
			pushf
			pop {}
			cli
		", out(reg) flags, options(preserves_flags, nomem))
		}

		flags & 0x0200
	}

	#[export_name = "__popcorn_set_irq"]
	fn set_interrupts(old_state: usize) {
		if old_state != 0 {
			unsafe { asm!("sti", options(preserves_flags, nomem)); }
		}
	}

	unsafe fn load_tls(ptr: *mut u8) {
		let tls_self_ptr_low = ptr as usize as u32;
		let tls_self_ptr_high = ((ptr as usize) >> 32) as u32;
		unsafe {
			asm!(
				"mov ecx, 0xc0000100", // ecx = FSBase MSR
				"wrmsr",
				in("edx") tls_self_ptr_high, in("eax") tls_self_ptr_low, out("ecx") _
			);
		}
	}

	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy) {
		paging2::construct_tables()
	}

	#[naked]
	unsafe extern "C" fn switch_thread(from: &mut ThreadControlBlock, to: &ThreadControlBlock) {
		asm!(
			"mov [rdi + {0}], rbx",
			"mov [rdi + {1}], rsp",
			"mov [rdi + {2}], rbp",
			"mov [rdi + {3}], r12",
			"mov [rdi + {4}], r13",
			"mov [rdi + {5}], r14",
			"mov [rdi + {6}], r15",

			"mov rax, [rsi + {7}]",
			"mov rcx, cr3",
			"cmp rax, rcx",
			"je 1f",
			"mov cr3, rax",
			"1:",

			// todo: adjust RSP0 in TSS
			"mov rbx, [rsi + {0}]",
			"mov rsp, [rsi + {1}]",
			"mov rbp, [rsi + {2}]",
			"mov r12, [rsi + {3}]",
			"mov r13, [rsi + {4}]",
			"mov r14, [rsi + {5}]",
			"mov r15, [rsi + {6}]",

			"ret",

			const offset_of!(Amd64SaveState, rbx) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, rsp) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, rbp) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, r12) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, r13) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, r14) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, r15) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(paging2::Amd64TTable, pml4) + offset_of!(ThreadControlBlock, ttable),
			options(noreturn)
		);
	}
}

#[derive(Debug, Default)]
struct Amd64SaveState {
	pub rbx: usize,
	pub rsp: usize,
	pub rbp: usize,
	pub r12: usize,
	pub r13: usize,
	pub r14: usize,
	pub r15: usize
}

impl SaveState for Amd64SaveState {
	fn new(tcb: &mut ThreadControlBlock, init: fn(), main: fn() -> !) -> Self {
		let stack = &mut tcb.kernel_stack;
		let stack_start = unsafe {
			let stack_top = stack.virtual_end().start().as_ptr().cast::<usize>();
			stack_top.sub(1).write(0xdeadbeef);
			stack_top.sub(2).write(0);
			stack_top.sub(3).write(main as usize);
			stack_top.sub(4).write(init as usize);
			stack_top.sub(4)
		};

		Self {
			rsp: stack_start as usize,
			.. Self::default()
		}
	}
}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
	warn!("BREAKPOINT: {frame:#x?}");
}
