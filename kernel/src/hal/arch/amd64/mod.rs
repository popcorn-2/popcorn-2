use core::alloc::Layout;
use core::arch::{asm, naked_asm};
use core::arch::x86_64::CpuidResult;
use core::fmt::Debug;
use core::mem::{offset_of, ManuallyDrop};
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use kernel_api::address_space::Kernel;
use kernel_api::allocator::AllocError;
use kernel_api::is_x86_feature_detected;
use kernel_api::mapping::{Mapping, Stack};
use kernel_api::memory::VirtualAddress;
use crate::hal;
use crate::hal::IpiTarget;
use crate::hal::{Hal, SaveStateTr, ThreadControlBlock};
use crate::hal::arch::amd64::msr::{rdmsr, wrmsr};
use crate::hal::interrupts_v2::Vector;
use crate::memory::paging::init_page_table;

mod gdt;
mod tss;
mod interrupts;
mod serial;
mod port;
mod qemu;
mod paging;
mod pic;

pub struct Amd64Hal;

unsafe impl Hal for Amd64Hal {
	type SerialOut = serial::HalWriter;
	type KTableTy = paging::Amd64KTable;
	type TTableTy = paging::Amd64TTable;
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

	fn early_init() -> Self::TTableTy {
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

		wrmsr(
			msr::STAR,
			((24 | 0b11) << 48) | (8 << 32),
		);
		wrmsr(
			msr::LSTAR,
			interrupts::amd64_syscall_handler as *const () as u64,
		);
		wrmsr(
			msr::SFMASK,
			0xED5, // IF - disabled
			       // OF, DF, SF, ZF, AF, PF, CF = 0 as required by SysV
		);

		interrupts::init_idt();
		pic::init();

		Self::enable_interrupts();

		if is_x86_feature_detected!("smep") {
			debug!("enabling SMEP");
			unsafe {
				asm!(
					"mov {0}, cr4",
					"or {0}, 0x100000",
					"mov cr4, {0}",
					out(reg) _
				)
			}
		}

		if is_x86_feature_detected!("smap") {
			debug!("enabling SMAP");
			unsafe {
				asm!(
					"mov {0}, cr4",
					"or {0}, 0x200000",
					"mov cr4, {0}",
					out(reg) _
				)
			}
		}

		if !is_x86_feature_detected!("xsave") {
			panic!("`xsave` not supported");
		}

		unsafe {
			asm!(
				"mov {0}, cr4",
				"or {0}, (1<<9) | (1<<10) | (1<<18)", // set OSFXSR, OSXMMEXCPT and OSXSAVE bits to signify to userspace that we support sse/avx
				"mov cr4, {0}",
				out(reg) _,
			)
		}

		let mut xcr0 = 0b11u32; // x87 and SSE enabed

		if is_x86_feature_detected!("xsave_avx") {
			xcr0 |= 1 << 2;
		}

		if is_x86_feature_detected!("xsave_avx512_opmask")
			&& is_x86_feature_detected!("xsave_avx512_zmm_hi16")
			&& is_x86_feature_detected!("xsave_avx512_zmm_hi256") {
			xcr0 |= 0b111 << 5;
		}

		unsafe {
			asm!("xsetbv", in("rcx") 0, in("eax") xcr0, in("edx") 0);
		}

		let (ktable, ttable) = unsafe { paging::construct_tables() };
		unsafe { init_page_table(ktable) };
		ttable
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
				pushfq
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
		wrmsr(msr::GS_BASE, ptr.addr() as _);
	}

	fn load_user_tls(ptr: *mut u8) {
		wrmsr(msr::FS_BASE, ptr.addr() as _);
	}

	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy) {
		unsafe { paging::construct_tables() }
	}

	#[inline]
	unsafe extern "C" fn switch_thread<'a>(from: &'a mut ManuallyDrop<ThreadControlBlock>, to: &ThreadControlBlock) -> &'a mut ManuallyDrop<ThreadControlBlock> {
		// currently in kernel mode so even if we get an interrupt on the new TSS.privilege_stack_table[0] value
		// the CPU won't pay attention to it
		tss::TSS.get().expect("TSS should be initialised")
				.set_rsp0(to.kernel_stack.as_ptr_range().end.into());
		percpu_v2!(kernel_stack_top).store(to.kernel_stack.as_ptr_range().end.cast_mut(), Ordering::Relaxed);
		
		from.register_state.fs = rdmsr(msr::FS_BASE) as usize;
		from.register_state.gs = rdmsr(msr::KERNEL_GS_BASE) as usize; // since we're in the kernel, KERNEL_GS_BASE will be the userspace gs
		wrmsr(msr::FS_BASE, to.register_state.fs as u64);
		wrmsr(msr::KERNEL_GS_BASE, to.register_state.gs as u64);

		debug!("rflags = {:#x}", to.register_state.rflags);

		#[cfg(kasan)] {
			let CpuidResult { ebx: xsave_size, .. } = core::arch::x86_64::__cpuid_count(0xd, 0);
			kernel_api::memory::asan::__asan_store_n(
				from.register_state.xsave.as_ptr().into(),
				xsave_size as usize,
			);
			kernel_api::memory::asan::__asan_load_n(
				to.register_state.xsave.as_ptr().into(),
				xsave_size as usize,
			);
		}

		return unsafe { inner(from, &to) };

		#[unsafe(naked)] // todo: convert to normal inline asm
		unsafe extern "C" fn inner<'a>(from: &'a mut ManuallyDrop<ThreadControlBlock>, to: &ThreadControlBlock) -> &'a mut ManuallyDrop<ThreadControlBlock> {
			// rdi: from -> rax
			// rsi: to
			naked_asm!(
				// save all registers into `from` Amd64SaveState struct
				"mov [rdi + {rbx_offset}], rbx",
				"mov [rdi + {rsp_offset}], rsp",
				"mov [rdi + {rbp_offset}], rbp",
				"mov [rdi + {r12_offset}], r12",
				"mov [rdi + {r13_offset}], r13",
				"mov [rdi + {r14_offset}], r14",
				"mov [rdi + {r15_offset}], r15",
				"pushfq",
				"pop rbx",
				"mov [rdi + {rflags_offset}], rbx",

				"xor ecx, ecx",
				"xgetbv",
				"mov rcx, [rdi + {xsave_ptr_offset}]",
				"xsave [rcx]",
	
				// restore all registers from `to` Amd64SaveState struct
				"mov rcx, [rsi + {xsave_ptr_offset}]",
				"xrstor [rcx]",
				"mov rbx, [rsi + {rflags_offset}]",
				"push rbx",
				"popfq",
				"mov rbx, [rsi + {rbx_offset}]",
				"mov rsp, [rsi + {rsp_offset}]",
				"mov rbp, [rsi + {rbp_offset}]",
				"mov r12, [rsi + {r12_offset}]",
				"mov r13, [rsi + {r13_offset}]",
				"mov r14, [rsi + {r14_offset}]",
				"mov r15, [rsi + {r15_offset}]",
				
				// move data from `from` into return register
				"mov rax, rdi",
	
				"ret",
	
				rbx_offset = const offset_of!(ThreadControlBlock, register_state.rbx),
				rsp_offset = const offset_of!(ThreadControlBlock, register_state.rsp),
				rbp_offset = const offset_of!(ThreadControlBlock, register_state.rbp),
				r12_offset = const offset_of!(ThreadControlBlock, register_state.r12),
				r13_offset = const offset_of!(ThreadControlBlock, register_state.r13),
				r14_offset = const offset_of!(ThreadControlBlock, register_state.r14),
				r15_offset = const offset_of!(ThreadControlBlock, register_state.r15),
				rflags_offset = const offset_of!(ThreadControlBlock, register_state.rflags),
				xsave_ptr_offset = const offset_of!(ThreadControlBlock, register_state.xsave),
			);
		}
	}

	fn send_ipi(_target: IpiTarget) -> Result<(), ()> {
		todo!()
	}
	
	fn send_local_eoi(_vector: Vector) {
		todo!()
		/*let xapic = unsafe { &*crate::hal::timing::local_timer().data().cast::<crate::hal::arch::apic::lapic::xapic::XApicTimer>() };
		xapic.0.eoi(vector);*/
	}

	#[inline]
	fn wait_for_interrupt() {
		unsafe {
			/*let val: u64;
			asm!(
				"pushfq",
				"pop {}",
				out(reg) val,
				options(preserves_flags, pure, nomem)
			);
			debug_assert!(val & 0x200 != 0, "should not `wfi` with interrupts disabled");*/
			asm!(
				"sti",
				"hlt",
				"cli",
				options(nostack, preserves_flags, nomem)
			);
		}
	}

	fn first_thread_init(tcb: &ThreadControlBlock) {
		let tss_rsp0 = tcb.kernel_stack.as_ptr_range().end.into();
		tss::TSS.get().expect("no TSS").set_rsp0(tss_rsp0);
	}

	extern "C" fn switch_to_userspace_at(addr: VirtualAddress, stack_top: VirtualAddress) -> ! {
		debug!("switch to userspace (rip: {addr:x?}, rsp: {stack_top:x?})");
		crate::hal::get_and_disable_interrupts(); // so we can switch stack without getting interrupted before we get to userspace
		unsafe {
			asm!(
					"swapgs",
					"mov rsp, {}", // return address from argument
					"mov r11, 0x3202", // fixme: HACK: IOPL = 3
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
	xsave: Xsave,
}

#[derive(Debug)]
struct Xsave {
	layout: Layout,
	allocation: NonNull<u8>,
}

impl Xsave {
	fn new() -> Self {
		let CpuidResult { ebx: xsave_size, .. } = core::arch::x86_64::__cpuid_count(0xd, 0);
		let layout = Layout::from_size_align(
			xsave_size as usize,
			64,
		).expect("invalid layout for xsave region");
		let allocation = unsafe {
			NonNull::new(alloc::alloc::alloc_zeroed(layout)) // we want the header to be zeroed, and it's easier to just zero the whole thing
				.expect("failed to allocate xsave region")
		};

		// fixme: mxcsr should be copied when spawning child thread, only reset for processes
		let mxcsr_val: u32 =
			1<<12 | // Precision masked
			1<<11 | // Underflow masked
			1<<10 | // Overflow masked
			1<<9  | // Underflow masked
			1<<8  | // Denormal masked
			1<<7;   // Invalid op masked

		unsafe {
			// fixme: set x87 control word too
			*allocation.as_ptr().byte_add(24).cast::<u32>() = mxcsr_val; // write initial MXCSR val to XSAVE header
			*allocation.as_ptr().byte_add(512).cast::<u64>() = 0b11;     // mark x87 and SSE state of XSTATE_BV as needing restore
		}

		Self {
			layout,
			allocation,
		}
	}
}

unsafe impl Send for Xsave {}

impl Drop for Xsave {
	fn drop(&mut self) {
		unsafe {
			alloc::alloc::dealloc(self.allocation.as_ptr(), self.layout);
		}
	}
}

impl Amd64SaveState {
	#[unsafe(naked)]
	unsafe extern "C" fn thread_startup() {
		naked_asm!(
			".cfi_startproc simple",
			".cfi_def_cfa rsp, 32",
			".cfi_offset rip, -32",
			"pop rbp", // aligns to 16 bytes
			".cfi_def_cfa rsp, 24",
			".cfi_register rip, rbp",
			"mov rdi, rax",
			".cfi_undefined rdi",
			"call {}",
			"pop rdi", // pop args off stack
			".cfi_def_cfa rsp, 16",
			"pop rdi",
			".cfi_def_cfa rsp, 8",
			"ret",
			".cfi_endproc",

			sym crate::threading::post_switch_cleanup
		);
	}
}

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
			xsave: Xsave::new(),
		}
	}
}

impl SaveStateTr for Amd64SaveState {
	fn new(stack: &mut Mapping<Stack, Kernel>, main: extern "C" fn(usize) -> !, arg: usize) -> Result<Self, AllocError> {
		let stack_start = unsafe {
			let stack_top = stack.as_mut_ptr_range().end.cast::<usize>();
			stack_top.sub(1).write(0);
			stack_top.sub(2).write(main as usize);
			stack_top.sub(3).write(arg); // Intentionally skip stack slot 4 here for alignment
			stack_top.sub(5).write(0);
			stack_top.sub(6).write(Self::thread_startup as *const () as usize);
			stack_top.sub(6)
		};

		Ok(Amd64SaveState {
			rsp: stack_start.addr(),
			.. Self::default()
		})
	}
}

pub(super) mod msr {
	use core::arch::asm;

	pub struct ModelSpecificRegister(usize);

	pub const IA32_APIC_BASE: ModelSpecificRegister = ModelSpecificRegister(0x1B);
	pub const IA32_TSC_DEADLINE: ModelSpecificRegister = ModelSpecificRegister(0x6e0);
	pub const STAR: ModelSpecificRegister = ModelSpecificRegister(0xC0000081);
	pub const LSTAR: ModelSpecificRegister = ModelSpecificRegister(0xC0000082);
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
