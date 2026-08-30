mod interrupts;
mod msr;
#[doc(hidden)]
pub mod tss;
mod gdt;
mod syscall;
mod pic;

use core::arch::asm;
pub use interrupts::get_and_disable_interrupts;
use kernel_api::memory::VirtualAddress;
use kernel_api::sync::LazyLock;

pub fn target_bsp_start() {
	get_and_disable_interrupts();
	percpu_v2!(arch).gdt.load();
	interrupts::IDT.load();
	syscall::init();
	pic::init();
	// enable SMAP/SMEP
	// enable and configure XSAVE if exists
}

pub struct Percpu {
	gdt: LazyLock<gdt::Gdt>,
	#[doc(hidden)]
	pub tss: tss::Tss,
}

impl Percpu {
	pub const fn new() -> Self {
		Self {
			gdt: gdt::Gdt::INIT,
			tss: tss::Tss::INIT,
		}
	}
}

pub fn switch_to_userspace(entrypoint: VirtualAddress) -> ! {
	debug!("switch to userspace @ {entrypoint:x?}");
	unsafe {
		asm!(
			"cli", // so we can swapgs without being interrupted
			"swapgs",
			// zero all registers to not leak to userspace
			"xor eax, eax",
			"xor ebx, ebx",
			"xor edx, edx",
			"xor esi, esi",
			"xor edi, edi",
			"xor esp, esp",
			"xor ebp, ebp",
			"xor r8d, r8d",
			"xor r9d, r9d",
			"xor r10d, r10d",
			"xor r12d, r12d",
			"xor r13d, r13d",
			"xor r14d, r14d",
			"xor r15d, r15d",
			"sysretq",
			in("rcx") entrypoint.addr,
			in("r11") 0x202, // enable interrupts, set reserved bit
			options(noreturn)
		)
	}
}
