#![no_std]
#![feature(derive_const)]
#![feature(const_mut_refs)]
#![feature(offset_of)]
#![feature(asm_const)]
#![feature(abi_x86_interrupt)]
#![feature(generic_const_exprs)]
#![feature(arbitrary_self_types)]
#![feature(fn_ptr_trait)]
#![feature(thread_local)]

#![feature(kernel_sync_once)]

// use lazy_static::lazy_static;
// use kernel_exports::{module_name, module_author, module_license};

use core::ops::{Index, IndexMut};
use crate::idt::handler::InterruptStackFrame;
use crate::idt::Idt;
use log::warn;
use crate::idt::entry::Type;

pub mod gdt;
mod tss;
pub mod idt;

/*module_name!("amd64 Hardware Abstraction Layer", "popcorn::hal::amd64");
module_author!("Eliyahu Gluschove-Koppel <popcorn@eliyahu.co.uk>");
module_license!("MPL-2.0");*/

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
