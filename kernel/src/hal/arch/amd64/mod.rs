use core::arch::{asm, global_asm};
use core::mem;
use core::mem::{MaybeUninit, offset_of};
use core::num::NonZeroU8;
use log::warn;
use crate::hal::ArgTuple;
use crate::hal::{Hal, SaveState, ThreadControlBlock};
use crate::hal::arch::amd64::idt::entry::Type;
use crate::hal::arch::amd64::idt::handler::InterruptStackFrame;
use crate::hal::arch::amd64::idt::Idt;
use crate::hal::arch::amd64::interrupts::entry::Type;
use crate::hal::arch::amd64::interrupts::handler::InterruptStackFrame;
use crate::hal::arch::amd64::interrupts::Idt;
use crate::hal::exception::{DebugTy, Exception, PageFault, Ty};
use crate::sprintln;

mod gdt;
mod tss;
mod interrupts;
mod serial;
mod port;
mod qemu;
mod paging2;
pub(crate) mod paging;
mod pic;

#[derive(Hal)]
struct Amd64Hal;

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

	const MIN_IRQ_NUM: usize = 48; // 0-32 for exceptions, 32-48 for masked pic
	const MAX_IRQ_NUM: usize = 255; // 255 for spurious apic
}

#[derive(Debug)]
struct Amd64SaveState {
	pub rbx: MaybeUninit<usize>,
	pub rsp: MaybeUninit<usize>,
	pub rbp: MaybeUninit<usize>,
	pub r12: MaybeUninit<usize>,
	pub r13: MaybeUninit<usize>,
	pub r14: MaybeUninit<usize>,
	pub r15: MaybeUninit<usize>
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
		}
	}
}

impl SaveState for Amd64SaveState {
	fn new<Args: ArgTuple>(tcb: &mut ThreadControlBlock, init: unsafe extern "C" fn(), main: extern "C" fn(Args) -> !, args: [MaybeUninit<usize>; 4]) -> Self {
		let stack = &mut tcb.kernel_stack;
		let stack_start = unsafe {
			let stack_top = stack.virtual_end().start().as_ptr().cast::<usize>();
			stack_top.sub(1).write(0xdeadbeef);
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
