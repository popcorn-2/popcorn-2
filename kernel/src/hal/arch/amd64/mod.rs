use core::arch::asm;
use crate::hal::IpiTarget;
use crate::hal::Hal;
use crate::hal::arch::amd64::msr::wrmsr;
use crate::hal::interrupts_v2::Vector;

mod gdt;
mod tss;
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

	const IPI_VECTOR: Vector = Vector(0x30);
	const SPURIOUS_VECTOR: Vector = Vector(0xFF);
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
