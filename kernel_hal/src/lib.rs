#![no_std]

#![feature(type_alias_impl_trait)]
#![feature(abi_x86_interrupt)]
#![feature(generic_const_exprs)]
#![feature(offset_of)]
#![feature(asm_const)]
#![feature(const_mut_refs)]

#![feature(kernel_sync_once)]

#[cfg(target_arch = "x86_64")]
pub mod amd64;

pub trait Hal {}
pub type CurrentHal = impl Hal;
