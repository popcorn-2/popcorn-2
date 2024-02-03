use core::arch::asm;
use log::warn;
use crate::Hal;
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
	fn get_and_disable_interrupts() -> bool {
		let flags: u64;
		unsafe {
			asm!("
			pushf
			pop {}
			cli
		", out(reg) flags, options(preserves_flags, nomem))
		}

		(flags & 0x0200) != 0
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
}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
	warn!("BREAKPOINT: {frame:#x?}");
}
