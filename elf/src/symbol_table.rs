use core::ffi::CStr;
use core::ptr::slice_from_raw_parts;
use log::debug;
use crate::dynamic_table::DynamicTableEntry;
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

impl<'a> super::File<'a> {
	pub fn dynamic_symbol_table(&self) -> Option<&[SymbolTableEntry]> {
		let symbol_table_addr = self.dynamic_table().find_map(|entry| match entry {
			DynamicTableEntry::SymbolTable(addr) => Some(addr),
			_ => None
		})?;

		let hash_table_addr = self.dynamic_table().find_map(|entry| match entry {
			DynamicTableEntry::HashTable(addr) => Some(addr),
			_ => None
		})?;

		let hashtable_length = unsafe { *self.data_at_address(hash_table_addr).unwrap().cast::<u32>().offset(1) };
		let symbol_table = {
			let symbol_table_ptr = self.data_at_address(symbol_table_addr).unwrap();
			unsafe { &*slice_from_raw_parts(symbol_table_ptr.cast::<SymbolTableEntry>(), usize::try_from(hashtable_length).unwrap()) }
		};

		Some(symbol_table)
	}
}
}
