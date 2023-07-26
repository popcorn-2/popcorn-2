use core::mem;
use core::ptr::slice_from_raw_parts;
use crate::ExecutableAddressRelocated;
use crate::header::program::SegmentType;
use super::ExecutableAddressUnrelocated;

mod private {
	pub trait Sealed {}
}
pub trait ExecutableAddress: private::Sealed {}
impl private::Sealed for ExecutableAddressRelocated {}
impl private::Sealed for ExecutableAddressUnrelocated {}
impl ExecutableAddress for ExecutableAddressRelocated {}
impl ExecutableAddress for ExecutableAddressUnrelocated {}

macro_rules! dynamic_table {
    ($(#[$attr:meta])* $vis:vis enum $name:ident with $addr_ty:ty) => {
	    $(#[$attr])*
	    #[repr(C, i64)]
	    $vis enum $name {
			Null = 0,
			NeededLibrary(u64) = 1,
			PltRelocationTableSize(u64) = 2,
			PltGot($addr_ty) = 3,
			HashTable($addr_ty) = 4,
			StringTable($addr_ty) = 5,
			SymbolTable($addr_ty) = 6,
			RelaTableAddress($addr_ty) = 7,
			RelaTableSize(u64) = 8,
			RelaTableEntrySize(u64) = 9,
			StringTableSize(u64) = 10,
			SymbolTableEntrySize(u64) = 11,
			InitFunction($addr_ty) = 12,
			TerminateFunction($addr_ty) = 13,
			ObjectName(u64) = 14,
			RPath(u64) = 15,
			Symbolic = 16,
			RelTableAddress($addr_ty) = 17,
			RelTableSize(u64) = 18,
			RelTableEntrySize(u64) = 19,
			PltRelType(u64) = 20,
			Debug = 21,
			TextRelocations = 22,
			JumptableRelocations($addr_ty) = 23,
		}
    };
}

dynamic_table! {
	#[derive(Debug, Copy, Clone)]
	pub enum DynamicTableEntryUnrel with ExecutableAddressUnrelocated
}

dynamic_table! {
	#[derive(Debug, Copy, Clone)]
	#[non_exhaustive]
	pub enum DynamicTableEntry with ExecutableAddressRelocated
}

impl DynamicTableEntryUnrel {
	unsafe fn relocate(self, base: u64) -> DynamicTableEntry {
		use DynamicTableEntry::*;

		match self {
			DynamicTableEntryUnrel::Null => Null,
			DynamicTableEntryUnrel::NeededLibrary(v) => NeededLibrary(v),
			DynamicTableEntryUnrel::PltRelocationTableSize(v) => PltRelocationTableSize(v),
			DynamicTableEntryUnrel::PltGot(v) => PltGot(v.relocate(base)),
			DynamicTableEntryUnrel::HashTable(v) => HashTable(v.relocate(base)),
			DynamicTableEntryUnrel::StringTable(v) => StringTable(v.relocate(base)),
			DynamicTableEntryUnrel::SymbolTable(v) => SymbolTable(v.relocate(base)),
			DynamicTableEntryUnrel::RelaTableAddress(v) => RelaTableAddress(v.relocate(base)),
			DynamicTableEntryUnrel::RelaTableSize(v) => RelaTableSize(v),
			DynamicTableEntryUnrel::RelaTableEntrySize(v) => RelaTableEntrySize(v),
			DynamicTableEntryUnrel::StringTableSize(v) => StringTableSize(v),
			DynamicTableEntryUnrel::SymbolTableEntrySize(v) => SymbolTableEntrySize(v),
			DynamicTableEntryUnrel::InitFunction(v) => InitFunction(v.relocate(base)),
			DynamicTableEntryUnrel::TerminateFunction(v) => TerminateFunction(v.relocate(base)),
			DynamicTableEntryUnrel::ObjectName(v) => ObjectName(v),
			DynamicTableEntryUnrel::RPath(v) => RPath(v),
			DynamicTableEntryUnrel::Symbolic => Symbolic,
			DynamicTableEntryUnrel::RelTableAddress(v) => RelTableAddress(v.relocate(base)),
			DynamicTableEntryUnrel::RelTableSize(v) => RelTableSize(v),
			DynamicTableEntryUnrel::RelTableEntrySize(v) => RelTableEntrySize(v),
			DynamicTableEntryUnrel::PltRelType(v) => PltRelType(v),
			DynamicTableEntryUnrel::Debug => Debug,
			DynamicTableEntryUnrel::TextRelocations => TextRelocations,
			DynamicTableEntryUnrel::JumptableRelocations(v) => JumptableRelocations(v.relocate(base))
		}
	}
}

const TAG_COUNT: i64 = 24;

pub struct DynamicTableIter<'a> {
	dynamic_table: core::slice::Iter<'a, (i64, u64)>
}

impl<'a> DynamicTableIter<'a> {
	pub(crate) fn new(dynamic_table: &[u8]) -> Self {
		let dynamic_table_ptr = dynamic_table.as_ptr().cast::<(i64, u64)>();
		if !dynamic_table_ptr.is_aligned() {
			panic!("Dynamic table not aligned");
		}

		// SAFETY: Checked alignment, and non-null since taken from slice
		Self {
			dynamic_table: unsafe { &*slice_from_raw_parts(dynamic_table_ptr, dynamic_table.len() / 16) }.iter()
		}
	}
}

impl<'a> Iterator for DynamicTableIter<'a> {
	type Item = Result<&'a DynamicTableEntryUnrel, ()>;

	fn next(&mut self) -> Option<Self::Item> {
		let tag = self.dynamic_table.next()?;

		if tag.0 < TAG_COUNT {
			// SAFETY: Checked tag value is within range
			Some(Ok(unsafe { mem::transmute::<_, &DynamicTableEntryUnrel>(tag) }))
		} else { Some(Err(())) }
	}
}

impl<'a> super::File<'a> {
	pub fn dynamic_table(&self) -> impl Iterator<Item = DynamicTableEntry> + '_ {
		let Some(dynamic_segment) = self.segments().find(|segment| segment.segment_type == SegmentType::DYNAMIC) else {
			panic!("can't find _DYNAMIC")
		};

		DynamicTableIter::new(self.index_data(dynamic_segment.file_location()))
				.filter_map(Result::ok)
				.map(|entry| unsafe { entry.relocate(self.base) })
	}
}
