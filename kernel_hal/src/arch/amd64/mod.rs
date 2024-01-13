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
pub(crate) mod paging;

#[derive(Hal)]
struct Amd64Hal;

unsafe impl Hal for Amd64Hal {
	type SerialOut = serial::HalWriter;

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
}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
	warn!("BREAKPOINT: {frame:#x?}");
}
