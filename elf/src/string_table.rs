use core::ffi;
use core::ffi::CStr;
use core::num::NonZeroU32;
use core::ptr::slice_from_raw_parts;

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
