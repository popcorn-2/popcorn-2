use core::ffi::CStr;
use core::ptr::slice_from_raw_parts;
use hashbrown::HashMap;
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

	pub fn exported_symbols(&self) -> SymbolMap<'_> {
		let mut map = SymbolMap::new();

		if let Some(symbol_table) = self.dynamic_symbol_table() {
			let string_table = self.dynamic_string_table().unwrap();

			for symbol in symbol_table {
				if symbol.section_table_index != 0 && !symbol.info.is_local() {
					let name = string_table.get_string(symbol.name.unwrap());
					debug!("{name:?} : {symbol:?}");
					// SAFETY: using correct base for this file
					let symbol =
							if symbol.info.is_weak() { ExportedSymbol::new_weak(unsafe { symbol.value.relocate(self.base) }, symbol.size) }
							else { ExportedSymbol::new_strong(unsafe { symbol.value.relocate(self.base) }, symbol.size) };
					map.insert(name, symbol);
				}
			}
		}

		map
	}
}

#[derive(Debug, Clone)]
pub struct SymbolMap<'a>(HashMap<&'a CStr, ExportedSymbol>);

impl<'a> SymbolMap<'a> {
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	pub fn get(&self, name: &CStr) -> Option<ExportedSymbol> {
		self.0.get(name).copied()
	}

	fn insert(&mut self, name: &'a CStr, addr: ExportedSymbol) {
		if addr.is_weak() {
			// ignore result since either
			// strong symbol already exists so don't need to overwrite
			// or weak symbol already exists so take first definition
			let _ = self.0.try_insert(name, addr);
		} else {
			// if inserting strong symbol
			// either successfully insert so do nothing
			// or weak symbol exists so overwrite it
			// or strong symbol already exists so use first definition
			if let Err(e) = self.0.try_insert(name, addr) {
				if e.value.is_weak() { self.0.insert(name, addr); }
			}
		}
	}
}

impl<'a> Default for SymbolMap<'a> {
	fn default() -> Self {
		Self::new()
	}
}

#[derive(Debug, Copy, Clone)]
pub struct ExportedSymbol {
	ty: ExportedSymbolTy,
	pub value: ExecutableAddressRelocated,
	pub size: u64
}

#[derive(Debug, Copy, Clone)]
pub enum ExportedSymbolTy {
	Strong,
	Weak
}

impl ExportedSymbol {
	pub fn new_strong(value: ExecutableAddressRelocated, size: u64) -> Self {
		Self {
			ty: ExportedSymbolTy::Strong,
			value,
			size
		}
	}

	pub fn new_weak(value: ExecutableAddressRelocated, size: u64) -> Self {
		Self {
			ty: ExportedSymbolTy::Weak,
			value,
			size
		}
	}

	pub fn is_strong(self) -> bool {
		match self.ty {
			ExportedSymbolTy::Strong => true,
			ExportedSymbolTy::Weak => false
		}
	}

	pub fn is_weak(self) -> bool { !self.is_strong() }
}
