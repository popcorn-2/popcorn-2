#[allow(unused_imports)] use crate::prelude::*;
use core::arch::{asm, naked_asm};
use core::fmt::{Debug, Formatter};
use core::mem::{MaybeUninit, offset_of};
use core::num::NonZero;
use core::sync::atomic::{AtomicUsize, Ordering};
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::{Frame, VirtualAddress};
use crate::hal::{ContextSwitchPreserve, IpiTarget};
use crate::hal::{Hal, SaveStateTr, ThreadControlBlock};
use crate::hal::arch::amd64::interrupts::handler::InterruptStackFrame;
use crate::hal::arch::amd64::msr::{rdmsr, wrmsr};
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

	#[inline]
	fn breakpoint() { unsafe { asm!("int3"); } }

	#[inline]
	fn exit(result: crate::hal::Result) -> ! {
		qemu::debug_exit(result)
	}

	#[inline]
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
			((24 | 0b11) << 48) | (8 << 32),
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

	#[inline]
	fn post_acpi_init() {
		super::apic::init();
	}

	#[inline]
	fn enable_interrupts() {
		unsafe { asm!("sti", options(preserves_flags)); }
	}

	#[inline]
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

	#[inline]
	fn set_interrupts(old_state: usize) {
		if old_state != 0 {
			unsafe { asm!("sti", options(preserves_flags)); }
		}
	}

	unsafe fn load_tls(ptr: *mut u8) {
		msr::wrmsr(msr::GS_BASE, ptr.addr() as _);
	}

	fn load_user_tls(ptr: *mut u8) {
		msr::wrmsr(msr::FS_BASE, ptr.addr() as _);
	}

	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy) {
		paging2::construct_tables()
	}

	#[inline]
	unsafe extern "C" fn switch_thread(from: &mut PointerView, to: &mut PointerView, preserve: ContextSwitchPreserve) -> ContextSwitchPreserve {
		// currently in kernel mode so even if we get an interrupt on the new TSS.privilege_stack_table[0] value
		// the CPU won't pay attention to it
		let ptr = tss::TSS.get().expect("TSS should be initialised")
				.set_rsp0(to.kernel_stack.virtual_valid_end().start().align_down());
		percpu_v2!(kernel_stack_top).store(to.kernel_stack.virtual_valid_end().start().addr, Ordering::Relaxed);
		
		from.save_state.fs = rdmsr(msr::FS_BASE) as usize;
		from.save_state.gs = rdmsr(msr::KERNEL_GS_BASE) as usize; // since we're in the kernel, KERNEL_GS_BASE will be the userspace gs
		wrmsr(msr::FS_BASE, to.save_state.fs as u64);
		wrmsr(msr::KERNEL_GS_BASE, to.save_state.gs as u64);
		
		return inner(from.save_state, to.save_state, preserve);

		#[unsafe(naked)] // todo: convert to normal inline asm
		unsafe extern "C" fn inner(from: &mut Amd64SaveState, to: &Amd64SaveState, preserve: ContextSwitchPreserve) -> ContextSwitchPreserve {
			// rdi: from
			// rsi: to
			// rdx: preserve.0 -> rax
			// rcx: preserve.1 -> rdx
			// r8: cr3
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

	#[inline]
	fn wait_for_interrupt() {
		unsafe {
			let val: u64;
			asm!(
				"pushfq",
				"pop {}",
				out(reg) val,
				options(preserves_flags, pure, nomem)
			);
			debug_assert!(val & 0x200 != 0, "should not `wfi` with interrupts disabled");
			asm!(
				"hlt",
				options(nostack, preserves_flags, nomem)
			);
		}
	}

	fn first_thread_init(tcb: &ThreadControlBlock) {
		let tss_rsp0 = tcb.kernel_stack.virtual_valid_end().start();
		tss::TSS.get().expect("no TSS").set_rsp0(tss_rsp0.align_down());
	}

	extern "C" fn switch_to_userspace_at(addr: VirtualAddress, stack_top: VirtualAddress) -> ! {
		debug!("switch to userspace (rip: {addr:x?}, rsp: {stack_top:x?})");
		crate::hal::get_and_disable_interrupts(); // so we can switch stack without getting interrupted before we get to userspace
		unsafe {
			asm!(
					"swapgs",
					"mov rsp, {}", // return address from argument
					"mov r11, 0x202",
					"sysretq",
					in(reg) stack_top.addr,
					in("rcx") addr.addr, // return address from argument
					options(noreturn)
			)
		}
	}

	const IPI_VECTOR: Vector = Vector(0x30);
	const SPURIOUS_VECTOR: Vector = Vector(0xFF);
}

#[derive(Debug)]
pub struct Amd64SaveState {
	pub rbx: usize,
	pub rsp: usize,
	pub rbp: usize,
	pub r12: usize,
	pub r13: usize,
	pub r14: usize,
	pub r15: usize,
	pub rflags: usize,
	pub fs: usize,
	pub gs: usize,
	//pub xsave: Box<XSaveArea>,
}

#[repr(align(64))]
struct XSaveArea([MaybeUninit<u8>]);

impl Default for Amd64SaveState {
	fn default() -> Self {
		Self {
			rbx: 0,
			rsp: 0,
			rbp: 0,
			r12: 0,
			r13: 0,
			r14: 0,
			r15: 0,
			fs: 0,
			gs: 0,
			// According to Sys V entry convention
			// Reserved bit 1 = 1
			// IE = 0
			rflags: 0x02,
		}
	}
}

impl SaveStateTr for Amd64SaveState {
	fn new(tcb: &mut ThreadControlBlock, init: unsafe extern "C" fn(), main: extern "C" fn(usize) -> !, arg: usize) -> Self {
		let stack = &mut tcb.kernel_stack;
		let stack_start = unsafe {
			let stack_top = stack.virtual_valid_end().start().as_ptr().cast::<usize>();
			stack_top.sub(1).write(0);
			stack_top.sub(2).write(main as usize);
			stack_top.sub(3).write(arg); // Intentionally skip stack slot 4 here for alignment
			stack_top.sub(5).write(0);
			stack_top.sub(6).write(init as usize);
			stack_top.sub(6)
		};

		Self {
			rsp: stack_start as usize,
			.. Self::default()
		}
	}

	unsafe fn set_entry(&mut self, main: extern "C" fn(usize) -> !, args: usize) {
		let stack_ptr = self.rsp as *mut usize;
		stack_ptr.offset(4).write(main as usize);
		stack_ptr.offset(3).write(args);
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
	pub const FS_BASE: ModelSpecificRegister = ModelSpecificRegister(0xC0000100);
	pub const GS_BASE: ModelSpecificRegister = ModelSpecificRegister(0xC0000101);
	pub const KERNEL_GS_BASE: ModelSpecificRegister = ModelSpecificRegister(0xC0000102);

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
