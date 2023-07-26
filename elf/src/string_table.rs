use core::ffi;
use core::ffi::CStr;
use core::num::NonZeroU32;
use core::ptr::slice_from_raw_parts;
use crate::dynamic_table::DynamicTableEntry;

pub struct StringTable<'a>(&'a [ffi::c_char]);

impl<'a> StringTable<'a> {
	pub fn get_string(&self, index: StringIndex) -> &'a CStr {
		assert!(usize::try_from(index.0.get()).unwrap() < self.0.len());
		unsafe { CStr::from_ptr(self.0.as_ptr().offset(isize::try_from(index.0.get()).unwrap())) }
	}
}

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct StringIndex(pub NonZeroU32);

impl From<u32> for StringIndex {
	fn from(value: u32) -> Self {
		Self(NonZeroU32::try_from(value).unwrap())
	}
}

impl<'a> super::File<'a> {
	pub fn dynamic_string_table(&self) -> Option<StringTable<'_>> {
		let string_table_addr = self.dynamic_table().find_map(|entry| match entry {
			DynamicTableEntry::StringTable(addr) => Some(addr),
			_ => None
		})?;

		let string_table_length = self.dynamic_table().find_map(|entry| match entry {
			DynamicTableEntry::StringTableSize(len) => Some(len),
			_ => None
		})?;

		let string_table = {
			let string_table_ptr = self.data_at_address(string_table_addr).unwrap();
			unsafe { &*slice_from_raw_parts(string_table_ptr.cast::<ffi::c_char>(), usize::try_from(string_table_length).unwrap()) }
		};

		Some(StringTable(string_table))
	}
}