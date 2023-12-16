use core::ffi::CStr;
use core::ptr;
use log::trace;
use lvgl_sys::{lv_label_create, lv_label_set_text, lv_obj_class_t, lv_obj_del, lv_obj_t, lv_textarea_add_text, lv_textarea_create, lv_textarea_set_text};

use crate::object::{obj_mut, Widget};

#[repr(transparent)]
pub struct Textarea {
	pub(crate) raw: *mut lv_obj_t
}

impl Textarea {
	pub fn new(parent: Option<obj_mut>) -> Self {
		let parent = parent.map_or(ptr::null_mut(), |p| p.raw);
		Self {
			raw: unsafe { lv_textarea_create(parent) },
		}
	}

	pub fn set_text(&mut self, text: &CStr) {
		unsafe {
			lv_textarea_set_text(self.raw, text.as_ptr())
		}
	}

	pub fn add_text(&mut self, text: &CStr) {
		unsafe {
			lv_textarea_add_text(self.raw, text.as_ptr())
		}
	}
}

unsafe impl Widget for Textarea {
	fn class() -> *const lv_obj_class_t {
		unsafe { &lvgl_sys::lv_textarea_class }
	}
}

impl Drop for Textarea {
	fn drop(&mut self) {
		trace!("textarea dropped");
		//unsafe { lv_obj_del(self.raw) }
	}
}
