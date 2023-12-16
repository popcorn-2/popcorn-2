use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use alloc::string::String;
use core::{ptr, str};
use core::ffi::CStr;
use log::trace;
use lvgl_sys::{lv_coord_t, lv_img_create, lv_img_dsc_t, lv_img_set_offset_x, lv_img_set_offset_y, lv_img_set_src, lv_label_set_text, lv_obj_class_t, lv_obj_del, lv_obj_t, LV_SYMBOL_DUMMY};

use crate::object::{obj_mut, Widget};

#[repr(transparent)]
pub struct Image {
	pub(crate) raw: *mut lv_obj_t
}

impl Image {
	pub fn new(parent: Option<obj_mut>) -> Self {
		let parent = parent.map_or(ptr::null_mut(), |p| p.raw);
		Self {
			raw: unsafe { lv_img_create(parent) },
		}
	}

	pub fn set_text(&mut self, text: &str) {
		let text = {
			let mut s = str::from_utf8(LV_SYMBOL_DUMMY).unwrap().to_owned();
			s.push_str(text);
			CString::new(s).unwrap()
		};

		unsafe {
			// LVGL internally does a strcpy so it's fine that `text` gets dropped
			lv_img_set_src(self.raw, text.as_ptr().cast());
		}
	}

	pub fn set_offset(&mut self, x: lv_coord_t, y: lv_coord_t) {
		unsafe {
			lv_img_set_offset_x(self.raw, x);
			lv_img_set_offset_y(self.raw, y);
		}
	}

	pub fn set_source(&mut self, src: ImageSource) {
		unsafe {
			lv_img_set_src(self.raw, <*const lv_img_dsc_t>::from(src).cast());
		}
	}
}

unsafe impl Widget for Image {
	fn class() -> *const lv_obj_class_t {
		unsafe { &lvgl_sys::lv_img_class }
	}
}

impl Drop for Image {
	fn drop(&mut self) {
		trace!("image dropped");
		//unsafe { lv_obj_del(self.raw) }
	}
}

#[derive(Copy, Clone, Debug)]
pub struct ImageSource<'a> {
	raw: &'a lv_img_dsc_t
}

impl ImageSource<'_> {
	pub fn new(img: &lv_img_dsc_t) -> ImageSource<'_> {
		ImageSource {
			raw: img
		}
	}
}

impl From<ImageSource<'_>> for *const lv_img_dsc_t {
	fn from(value: ImageSource<'_>) -> Self {
		value.raw
	}
}