#[allow(unused_imports)] use crate::prelude::*;

#[cfg(target_arch = "x86_64")]
pub mod amd64;

#[cfg(target_arch = "x86_64")]
pub(super) use amd64::Amd64Hal as Arch;

#[cfg(target_arch = "x86_64")]
pub mod apic;

#[cfg(target_arch = "x86_64")]
pub mod hpet;
