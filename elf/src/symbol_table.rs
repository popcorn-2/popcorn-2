use core::ffi::CStr;
use core::ptr::slice_from_raw_parts;
use log::debug;
use crate::{ExecutableAddressRelocated, ExecutableAddressUnrelocated};
use super::string_table::StringIndex;

#[derive(Debug)]
#[repr(C)]
pub struct SymbolTableEntry {
	pub name: Option<StringIndex>,
	pub info: SymbolInfo,
	other: u8,
	pub section_table_index: u16,
	pub value: ExecutableAddressUnrelocated,
	pub size: u64
}

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct SymbolInfo(u8);

#[allow(unused)]
impl SymbolInfo {
	const LOCAL: u8 = 0;
	const GLOBAL: u8 = 1;
	const WEAK: u8 = 2;

	pub fn is_local(&self) -> bool { self.get_binding() == Self::LOCAL }
	pub fn is_weak(&self) -> bool { self.get_binding() == Self::WEAK }
	fn get_binding(&self) -> u8 { self.0 & 0xf }
	fn get_type(&self) -> u8 { self.0 >> 4 }
}
