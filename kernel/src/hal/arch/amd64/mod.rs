#[allow(unused_imports)] use crate::prelude::*;
use core::arch::{asm, naked_asm};
use core::fmt::{Debug, Formatter};
use core::mem::{MaybeUninit, offset_of};
use core::num::NonZero;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::VirtualAddress;
use crate::hal::{ContextSwitchPreserve, IpiTarget};
use crate::hal::{Hal, SaveStateTr, ThreadControlBlock};
use crate::hal::arch::amd64::interrupts::handler::InterruptStackFrame;
use crate::hal::interrupts_v2::Vector;
use crate::hal::timing::TimerMeta;
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

		msr::wrmsr(
			msr::STAR,
			((24 | 0b11) << 48) | (8 << 48),
		);
		msr::wrmsr(
			msr::LSTAR,
			interrupts::amd64_syscall_handler as u64,
		);
		msr::wrmsr(
			msr::SFMASK,
			0xED5, // IF - disabled
			       // OF, DF, SF, ZF, AF, PF, CF = 0 as required by SysV
		);

		interrupts::init_idt();
		pic::init();

		Self::enable_interrupts();
	}

	fn post_acpi_init() {
		super::apic::init();
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
		msr::wrmsr(msr::GSBase, ptr.addr() as _);
	}

	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy) {
		paging2::construct_tables()
	}
	
	unsafe extern "C" fn switch_thread(from: &mut PointerView, to: &mut PointerView, preserve: ContextSwitchPreserve) -> ContextSwitchPreserve {
		// currently in kernel mode so even if we get an interrupt on the new TSS.privilege_stack_table[0] value
		// the CPU won't pay attention to it
		tss::TSS.get().expect("TSS should be initialised")
				.set_rsp0(to.kernel_stack.virtual_end().end().align_down());
		
		return inner(from.save_state, to.save_state, preserve);
		
		#[naked]
		unsafe extern "C" fn inner(from: &mut Amd64SaveState, to: &Amd64SaveState, preserve: ContextSwitchPreserve) -> ContextSwitchPreserve {
			// rdi: from
			// rsi: to
			// rdx: preserve.0 -> rax
			// rcx: preserve.1 -> rdx
			naked_asm!(
				// save all registers into `from` Amd64SaveState struct
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
	
				//todo: "mov r12, [rsi + {pml4_offset}]",
				//"mov r13, cr3",
				//"cmp r12, r13",
				//"je 2f",
				//"mov cr3, r12",
				//"2:",
	
				// restore all registers from `to` Amd64SaveState struct
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
				
				// move data from `ContextSwitchPreserve` into return registers
				"mov rax, rdx",
				"mov rdx, rcx",
	
				"ret",
	
				rbx_offset = const offset_of!(Amd64SaveState, rbx),
				rsp_offset = const offset_of!(Amd64SaveState, rsp),
				rbp_offset = const offset_of!(Amd64SaveState, rbp),
				r12_offset = const offset_of!(Amd64SaveState, r12),
				r13_offset = const offset_of!(Amd64SaveState, r13),
				r14_offset = const offset_of!(Amd64SaveState, r14),
				r15_offset = const offset_of!(Amd64SaveState, r15),
				rflags_offset = const offset_of!(Amd64SaveState, rflags),
				//pml4_offset = const offset_of!(PointerView, ttable.pml4),
			);
		}
	}

	fn send_ipi(target: IpiTarget) -> Result<(), ()> {
		todo!()
	}
	
	fn send_local_eoi(vector: Vector) {
		let xapic = unsafe { &*crate::hal::timing::local_timer().data().cast::<crate::hal::arch::apic::lapic::xapic::XApicTimer>() };
		xapic.0.eoi(vector);
	}

	fn wait_for_interrupt() {
		unsafe {
			asm!(
				"sti",
				"hlt",
				options(nostack, preserves_flags)
			);
		}
	}

	fn first_thread_init(tcb: &ThreadControlBlock) {
		let tss_rsp0 = tcb.kernel_stack.virtual_end().end();
		tss::TSS.get().expect("no TSS").set_rsp0(tss_rsp0.align_down());
	}

	#[naked]
	extern "C" fn switch_to_userspace_at(addr: VirtualAddress) -> ! {
		unsafe {
			naked_asm!(
					"mov rcx, rdi", // return address from argument
					"mov r11, 0x202",
					"sysretq"
			)
		}
	}

	const IPI_VECTOR: Vector = Vector(0x30);
	const SPURIOUS_VECTOR: Vector = Vector(0xFF);
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
	fn new(tcb: &mut ThreadControlBlock, init: unsafe extern "C" fn(), main: extern "C" fn(usize) -> !, arg: usize) -> Self {
		let stack = &mut tcb.kernel_stack;
		let stack_start = unsafe {
			let stack_top = stack.virtual_end().start().as_ptr().cast::<usize>();
			stack_top.sub(1).write(0);
			stack_top.sub(2).write(main as usize);
			stack_top.sub(3).write(arg); // Intentionally skip stack slot 4 here for alignment
			stack_top.sub(5).write(0);
			stack_top.sub(6).write(init as usize);
			stack_top.sub(6)
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

pub(super) mod msr {
	use core::arch::asm;

	pub struct ModelSpecificRegister(usize);

	pub const IA32_APIC_BASE: ModelSpecificRegister = ModelSpecificRegister(0x1B);
	pub const IA32_TSC_DEADLINE: ModelSpecificRegister = ModelSpecificRegister(0x6e0);
	pub const STAR: ModelSpecificRegister = ModelSpecificRegister(0xC0000081);
	pub const LSTAR: ModelSpecificRegister = ModelSpecificRegister(0xC0000082);
	pub const CSTAR: ModelSpecificRegister = ModelSpecificRegister(0xC0000083);
	pub const SFMASK: ModelSpecificRegister = ModelSpecificRegister(0xC0000084);
	pub const GSBase: ModelSpecificRegister = ModelSpecificRegister(0xc0000101);

	// fixme: is this always safe?
	pub fn rdmsr(msr: ModelSpecificRegister) -> u64 {
		let (low, high): (u32, u32);
		unsafe {
			asm!("rdmsr", in("ecx") msr.0, out("eax") low, out("edx") high, options(nostack, nomem, preserves_flags));
		}

		u64::from(low) | u64::from(high) << 32
	}

	// fixme: is this always safe?
	pub fn wrmsr(msr: ModelSpecificRegister, val: u64) {
		let low = val as u32;
		let high = (val >> 32) as u32;
		unsafe {
			asm!("wrmsr", in("rcx") msr.0, in("eax") low, in("edx") high, options(nostack, nomem, preserves_flags));
		}
	}
}
