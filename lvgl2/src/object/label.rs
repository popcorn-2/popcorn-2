use core::ffi::CStr;
use core::ptr;
use log::trace;
use lvgl_sys::{lv_label_create, lv_label_set_recolor, lv_label_set_text, lv_obj_class_t, lv_obj_del, lv_obj_t};

use crate::object::{obj_mut, Widget};

#[repr(transparent)]
pub struct Label {
	pub(crate) raw: *mut lv_obj_t
}

impl Label {
	pub fn new(parent: Option<obj_mut>) -> Self {
		let parent = parent.map_or(ptr::null_mut(), |p| p.raw);
		Self {
			raw: unsafe { lv_label_create(parent) },
		}
	}

	pub fn set_text(&mut self, text: &CStr) {
		unsafe {
			lv_label_set_text(self.raw, text.as_ptr())
		}
	}

	pub fn set_recolor(&mut self, enable: bool) {
		unsafe {
			lv_label_set_recolor(self.raw, enable);
		}
	}
}

unsafe impl Widget for Label {
	fn class() -> *const lv_obj_class_t {
		unsafe { &lvgl_sys::lv_label_class }
	}
}

impl Drop for Label {
	fn drop(&mut self) {
		trace!("label dropped");
		//unsafe { lv_obj_del(self.raw) }
	}
}
