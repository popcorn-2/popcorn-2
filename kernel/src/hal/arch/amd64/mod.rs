#[allow(unused_imports)] use crate::prelude::*;
use core::arch::{asm, naked_asm};
use core::fmt::{Debug, Formatter};
use core::mem::{MaybeUninit, offset_of};
use core::num::NonZero;
use kernel_api::memory::mapping::Stack;
use crate::hal::{ArgTuple, ContextSwitchPreserve};
use crate::hal::{Hal, SaveStateTr, ThreadControlBlock};
use crate::hal::arch::amd64::interrupts::handler::InterruptStackFrame;
use crate::threading::{ThreadPointer, PointerView};

mod gdt;
mod tss;
mod interrupts;
mod serial;
mod port;
mod qemu;
mod paging2;
pub(crate) mod paging;
mod pic;

pub struct Amd64Hal;

unsafe impl Hal for Amd64Hal {
	type SerialOut = serial::HalWriter;
	type KTableTy = paging2::Amd64KTable;
	type TTableTy = paging2::Amd64TTable;
	type SaveState = Amd64SaveState;
	type LocalTimer = super::apic::LapicTimer;

	fn breakpoint() { unsafe { asm!("int3"); } }

	fn exit(result: crate::hal::Result) -> ! {
		qemu::debug_exit(result)
	}

	fn debug_output(data: &[u8]) -> Result<(), ()> {
		qemu::debug_con_write(data);
		Ok(())
	}

	fn early_init() {
		let tss = tss::TSS.get_or_init(|| {
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

		interrupts::init_idt();
		pic::init();

		Self::enable_interrupts();
	}

	fn post_acpi_init() {
		super::apic::init(0xff);
	}

	fn enable_interrupts() {
		unsafe { asm!("sti", options(preserves_flags)); }
	}

	fn get_and_disable_interrupts() -> usize {
		let flags: usize;
		unsafe {
			asm!("
			pushf
			pop {}
			cli
		", out(reg) flags, options(preserves_flags))
		}

		flags & 0x0200
	}

	fn set_interrupts(old_state: usize) {
		if old_state != 0 {
			unsafe { asm!("sti", options(preserves_flags)); }
		}
	}

	unsafe fn load_tls(ptr: *mut u8) {
		let tls_self_ptr_low = ptr as usize as u32;
		let tls_self_ptr_high = ((ptr as usize) >> 32) as u32;
		unsafe {
			asm!(
				"wrmsr",
				in("edx") tls_self_ptr_high, in("eax") tls_self_ptr_low, in("ecx") 0xc0000100u32 // FSBase MSR
			);
		}
	}

	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy) {
		paging2::construct_tables()
	}

	#[naked]
	unsafe extern "C" fn switch_thread(from: &PointerView, to: &PointerView, preserve: ContextSwitchPreserve) -> ContextSwitchPreserve {
		// rdi: from
		// rsi: to
		// rdx: preserve.0 -> rax
		// rcx: preserve.1 -> rdx
		naked_asm!(
			"mov rdi, [rdi + {save_state_ptr_offset}]", // load pointer to `from` save-state into `rdi`
			"mov rsi, [rsi + {save_state_ptr_offset}]", // load pointer to `to` save-state into `rsi`

			"mov [rdi + {rbx_offset}], rbx",
			"mov [rdi + {rsp_offset}], rsp",
			"mov [rdi + {rbp_offset}], rbp",
			"mov [rdi + {r12_offset}], r12",
			"mov [rdi + {r13_offset}], r13",
			"mov [rdi + {r14_offset}], r14",
			"mov [rdi + {r15_offset}], r15",
			"pushf",
			"pop rbx",
			"mov [rdi + {rflags_offset}], rbx",

			"mov r12, [rsi + {pml4_offset}]",
			"mov r13, cr3",
			"cmp r12, r13",
			"je 2f",
			"mov cr3, r12",
			"2:",

			// todo: adjust RSP0 in TSS
			"mov rbx, [rdi + {rflags_offset}]",
			"push rbx",
			"popf",
			"mov rbx, [rsi + {rbx_offset}]",
			"mov rsp, [rsi + {rsp_offset}]",
			"mov rbp, [rsi + {rbp_offset}]",
			"mov r12, [rsi + {r12_offset}]",
			"mov r13, [rsi + {r13_offset}]",
			"mov r14, [rsi + {r14_offset}]",
			"mov r15, [rsi + {r15_offset}]",

			"mov rax, rdx",
			"mov rdx, rcx",

			"ret",

			save_state_ptr_offset = const offset_of!(PointerView, save_state),
			rbx_offset = const offset_of!(ThreadControlBlock, save_state.rbx),
			rsp_offset = const offset_of!(ThreadControlBlock, save_state.rsp),
			rbp_offset = const offset_of!(ThreadControlBlock, save_state.rbp),
			r12_offset = const offset_of!(ThreadControlBlock, save_state.r12),
			r13_offset = const offset_of!(ThreadControlBlock, save_state.r13),
			r14_offset = const offset_of!(ThreadControlBlock, save_state.r14),
			r15_offset = const offset_of!(ThreadControlBlock, save_state.r15),
			rflags_offset = const offset_of!(ThreadControlBlock, save_state.rflags),
			pml4_offset = const offset_of!(ThreadControlBlock, ttable.pml4),
		);
	}

	const MIN_IRQ_NUM: usize = 48; // 0-32 for exceptions, 32-48 for masked pic
	const MAX_IRQ_NUM: usize = 255; // 255 for spurious apic
}

pub struct Amd64SaveState {
	pub rbx: MaybeUninit<usize>,
	pub rsp: MaybeUninit<usize>,
	pub rbp: MaybeUninit<usize>,
	pub r12: MaybeUninit<usize>,
	pub r13: MaybeUninit<usize>,
	pub r14: MaybeUninit<usize>,
	pub r15: MaybeUninit<usize>,
	pub rflags: MaybeUninit<usize>,
}

impl Debug for Amd64SaveState {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Amd64SaveState")
				.field("rbx", unsafe { &self.rbx.assume_init_read() })
				.field("rsp", unsafe { &self.rsp.assume_init_read() })
				.field("rbp", unsafe { &self.rbp.assume_init_read() })
				.field("r12", unsafe { &self.r12.assume_init_read() })
				.field("r13", unsafe { &self.r13.assume_init_read() })
				.field("r14", unsafe { &self.r14.assume_init_read() })
				.field("r15", unsafe { &self.r15.assume_init_read() })
				.field("rflags", unsafe { &self.rflags.assume_init_read() })
				.finish()
	}
}

impl Default for Amd64SaveState {
	fn default() -> Self {
		Self {
			rbx: MaybeUninit::zeroed(),
			rsp: MaybeUninit::zeroed(),
			rbp: MaybeUninit::zeroed(),
			r12: MaybeUninit::zeroed(),
			r13: MaybeUninit::zeroed(),
			r14: MaybeUninit::zeroed(),
			r15: MaybeUninit::zeroed(),
			// According to Sys V entry convention
			// Reserved bit 1 = 1
			// IE = 0
			rflags: MaybeUninit::new(0x02),
		}
	}
}

impl SaveStateTr for Amd64SaveState {
	fn new<Args: ArgTuple>(tcb: &mut ThreadControlBlock, init: unsafe extern "C" fn(), main: extern "C" fn(Args) -> !, args: [MaybeUninit<usize>; 4]) -> Self {
		let stack = &mut tcb.kernel_stack;
		let stack_start = unsafe {
			let stack_top = stack.virtual_end().start().as_ptr().cast::<usize>();
			stack_top.sub(1).write(0);
			stack_top.sub(2).write(main as usize);
			stack_top.sub(3).cast::<MaybeUninit<_>>().write(args[3]);
			stack_top.sub(4).cast::<MaybeUninit<_>>().write(args[2]);
			stack_top.sub(5).cast::<MaybeUninit<_>>().write(args[1]);
			stack_top.sub(6).cast::<MaybeUninit<_>>().write(args[0]);
			stack_top.sub(7).write(0);
			stack_top.sub(8).write(init as usize);
			stack_top.sub(8)
		};

		Self {
			rsp: MaybeUninit::new(stack_start as usize),
			.. Self::default()
		}
	}
}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
	warn!("BREAKPOINT: {frame:#x?}");
}
