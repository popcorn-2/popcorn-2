#![no_std]
#![feature(derive_const)]
#![feature(const_mut_refs)]
#![feature(offset_of)]
#![feature(asm_const)]

use lazy_static::lazy_static;
use kernel_exports::{module_name, module_author, module_license};

mod gdt;
mod tss;
