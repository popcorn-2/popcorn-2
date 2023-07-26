#![no_std]
#![feature(derive_const)]
#![feature(const_mut_refs)]
#![feature(rustc_attrs)]
#![feature(offset_of)]
#![feature(asm_const)]

use lazy_static::lazy_static;
use kernel_exports::{module_name, module_author, module_license};

mod gdt;
mod tss;
mod interrupts;

module_name!("amd64 Hardware Abstraction Layer", "popcorn::hal::amd64");
module_author!("Eliyahu Gluschove-Koppel <popcorn@eliyahu.co.uk>");
module_license!("MPL-2.0");

#[no_mangle]
pub fn main() {
    gdt::init_gdt();
}
