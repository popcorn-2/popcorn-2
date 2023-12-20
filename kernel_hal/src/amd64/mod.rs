use log::warn;
use crate::amd64::idt::entry::Type;
use crate::amd64::idt::handler::InterruptStackFrame;
use crate::amd64::idt::Idt;

pub mod gdt;
mod tss;
pub mod idt;

struct Hal;

impl super::Hal for Hal {}

const _: super::CurrentHal = Hal;

#[no_mangle]
pub fn __popcorn_hal_early_init() {
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

	let idt = idt::IDT.get_or_init(|| {
		let mut idt = Idt::new();
		idt.breakpoint = idt::entry::Entry::new(breakpoint, None, 3, Type::InterruptGate);
		idt
	});
	idt.load();
}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
	warn!("BREAKPOINT: {frame:#x?}");
}
