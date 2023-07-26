use core::ffi::CStr;
use core::iter::zip;
use core::mem;
use core::mem::size_of;
use core::ptr::slice_from_raw_parts;
use num_enum::TryFromPrimitive;
use crate::{ExecutableAddressUnrelocated};
use super::dynamic_table::DynamicTableEntry;

#[derive(Debug)]
pub enum RelocationTableEntry {
	Rel(Relocation),
	Rela(RelocationWithAddend)
}

impl RelocationTableEntry {
	pub const fn info(&self) -> u64 {
		match self {
			RelocationTableEntry::Rel(Relocation{info, ..}) => *info,
			RelocationTableEntry::Rela(RelocationWithAddend{info, ..}) => *info
		}
	}

	pub const fn symbol_table_index(&self) -> u32 {
		(self.info() >> 32) as u32
	}

	pub fn symbol_table_type(&self) -> RelocationType {
		RelocationType::try_from(self.info() & 0xffff_ffff).expect("Unsupported relocation type")
	}
}

#[allow(non_camel_case_types)]
#[derive(TryFromPrimitive, Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u64)]
pub enum RelocationType {
	X86_64_Relative = 8,
	X86_64_JumpSlot = 7
}

#[derive(Debug, Eq, PartialEq)]
pub enum RelocationEntryType {
	Rel,
	Rela,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Relocation {
	offset: ExecutableAddressUnrelocated,
	info: u64
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct RelocationWithAddend {
	offset: ExecutableAddressUnrelocated,
	info: u64,
	pub addend: i64
}

impl<'a> super::File<'a> {
	pub fn relocations(&self) -> impl Iterator<Item = RelocationTableEntry> + '_ {
		let rel_table_addresses = self.dynamic_table().filter_map(|entry| match entry {
			DynamicTableEntry::RelTableAddress(addr) => Some(addr),
			_ => None
		});

		let rel_table_lengths = self.dynamic_table().filter_map(|entry| match entry {
			DynamicTableEntry::RelTableSize(len) => Some(len),
			_ => None
		});

		let rel_tables = zip(rel_table_addresses, rel_table_lengths);
		let rel_tables = rel_tables.map(|(start, length)| {
			let rel_table_ptr = self.data_at_address(start).unwrap();
			let slice = unsafe { &*slice_from_raw_parts(rel_table_ptr.cast::<Relocation>(), usize::try_from(length).unwrap() / mem::size_of::<Relocation>()) };
			slice.iter()
			     .map(|entry| RelocationTableEntry::Rel(*entry))
		});

		let rela_table_addresses = self.dynamic_table().filter_map(|entry| match entry {
			DynamicTableEntry::RelaTableAddress(addr) => Some(addr),
			_ => None
		});

		let rela_table_lengths = self.dynamic_table().filter_map(|entry| match entry {
			DynamicTableEntry::RelaTableSize(len) => Some(len),
			_ => None
		});

		let rela_tables = zip(rela_table_addresses, rela_table_lengths);
		let rela_tables = rela_tables.map(|(start, length)| {
			let rela_table_ptr = self.data_at_address(start).unwrap();
			let slice = unsafe { &*slice_from_raw_parts(rela_table_ptr.cast::<RelocationWithAddend>(), usize::try_from(length).unwrap() / mem::size_of::<RelocationWithAddend>()) };
			slice.iter()
			     .map(|entry| RelocationTableEntry::Rela(*entry))
		});

		rel_tables.flatten().chain(rela_tables.flatten())
	}

	pub fn jumptable_relocations(&self) -> Option<impl Iterator<Item = RelocationTableEntry> +'_> {
		let table_address = self.dynamic_table().find_map(|entry| match entry {
			DynamicTableEntry::JumptableRelocations(addr) => Some(addr),
			_ => None
		})?;

		let table_length = self.dynamic_table().find_map(|entry| match entry {
			DynamicTableEntry::PltRelocationTableSize(len) => Some(len),
			_ => None
		})?;

		let table_entry_type = self.dynamic_table().find_map(|entry| match entry {
			DynamicTableEntry::PltRelType(7) => Some(RelocationEntryType::Rela),
			DynamicTableEntry::PltRelType(17) => Some(RelocationEntryType::Rel),
			_ => None
		})?;

		let table_pointer = self.data_at_address(table_address).unwrap();

		let a = if table_entry_type == RelocationEntryType::Rel {
			let slice = unsafe { &*slice_from_raw_parts(
				table_pointer.cast::<Relocation>(),
				usize::try_from(table_length).unwrap() / size_of::<Relocation>()
			) };
			Some(slice.iter().map(|entry| RelocationTableEntry::Rel(*entry)))
		} else { None };

		let b = if table_entry_type == RelocationEntryType::Rela {
			let slice = unsafe { &*slice_from_raw_parts(
				table_pointer.cast::<RelocationWithAddend>(),
				usize::try_from(table_length).unwrap() / size_of::<RelocationWithAddend>()
			) };
			Some(slice.iter().map(|entry| RelocationTableEntry::Rela(*entry)))
		} else { None };

		Some(a.into_iter().flatten().chain(b.into_iter().flatten()))
	}

	pub fn relocate(&mut self, base: u64) {
		let relocs = self.relocations().collect::<alloc::vec::Vec<_>>();
		for reloc in relocs {
			let RelocationTableEntry::Rela(RelocationWithAddend{ offset: addr, info: _, addend }) = reloc else {
				// TODO: MULTIARCH
				unreachable!("amd64 only supports Rela")
			};

			let ptr: *mut u8 = self.data_at_unrel_address_mut(addr).unwrap();
			match reloc.symbol_table_type() {
				RelocationType::X86_64_Relative => unsafe {
					*ptr.cast::<u64>() = base.wrapping_add_signed(addend)
				}
				_ => continue
			}
		}

		self.base = base;
	}

	pub fn link(&mut self, symbol_map: &crate::symbol_table::SymbolMap) -> Result<(), LinkError<'_>> {
		let symbol_table = self.dynamic_symbol_table().unwrap();
		let string_table = self.dynamic_string_table().unwrap();
		let plt = self.jumptable_relocations().unwrap();

		for i in plt {
			let RelocationTableEntry::Rela(RelocationWithAddend{ offset: addr, .. }) = i else {
				todo!("Only `Rela` table type supported")
			};

			let idx = i.symbol_table_index();
			let name = string_table.get_string(symbol_table[usize::try_from(idx).unwrap()].name.unwrap());

			let address = symbol_map.get(name).ok_or(LinkError(name))?;

			let ptr: *mut u8 = self.data_at_unrel_address(addr).unwrap() as *mut u8;
			match i.symbol_table_type() {
				RelocationType::X86_64_JumpSlot => unsafe {
					*ptr.cast::<u64>() = address.value.0
				}
				_ => todo!("Only x86_64 supported")
			}
		}

		Ok(())
	}
}

#[derive(Debug)]
pub struct LinkError<'a>(&'a CStr);

impl<'a> LinkError<'a> {
	pub fn name(&self) -> &CStr {
		self.0
	}
}
