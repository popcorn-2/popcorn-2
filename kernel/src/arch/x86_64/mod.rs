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
use kernel_api::is_x86_feature_detected;
use kernel_api::sync::LazyLock;

pub fn target_bsp_start() {
	get_and_disable_interrupts();
	percpu_v2!(arch).gdt.load();
	interrupts::IDT.load();
	syscall::init();
	pic::init();

	let mut cr4_bits = 0u64;
	if is_x86_feature_detected!("smep") {
		debug!("enabling SMEP");
		cr4_bits |= 1 << 20;
	}

	if is_x86_feature_detected!("smap") {
		debug!("enabling SMAP");
		cr4_bits |= 1 << 21;
	}

	if !is_x86_feature_detected!("xsave") {
		panic!("XSAVE support is required");
	}

	debug!("enabling XSAVE (x87 + SSE)");
	cr4_bits |= 1 << 9; // OS fxsave/fxrstor support
	cr4_bits |= 1 << 10; // OS SIMD exception support
	cr4_bits |= 1 << 18; // OS xsave support

	let mut xcr0 = 0b11u32; // x87 and SSE enabed

	if is_x86_feature_detected!("xsave_avx") {
		debug!("enabling AVX");
		xcr0 |= 1 << 2;
	}

	if is_x86_feature_detected!("xsave_avx512_opmask")
		&& is_x86_feature_detected!("xsave_avx512_zmm_hi16")
		&& is_x86_feature_detected!("xsave_avx512_zmm_hi256") {
		debug!("enabling AVX-512");
		xcr0 |= 0b111 << 5;
	}

	if is_x86_feature_detected!("xsave_amx_cfg")
		&& is_x86_feature_detected!("xsave_amx_tile_data") {
		debug!("enabling AMX");
		xcr0 |= 0b11 << 17;
	}

	if is_x86_feature_detected!("xsave_apx_gpr") {
		debug!("enabling APX");
		xcr0 |= 1 << 19;
	}

	unsafe {
		asm!(
			"mov {scratch}, cr4",
			"or {scratch}, {val}",
			"mov cr4, {scratch}",
			val = in(reg) cr4_bits,
			scratch = out(reg) _,
		);

		asm!("xsetbv", in("rcx") 0, in("eax") xcr0, in("edx") 0);
	}
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
