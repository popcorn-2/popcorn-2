use core::ffi::CStr;
use core::ptr;
use core::time::Duration;
use log::trace;
use lvgl_sys::{lv_label_create, lv_label_set_text, lv_obj_class_t, lv_obj_del, lv_obj_t, lv_spinner_create};

use crate::object::{obj_mut, Widget};

#[repr(transparent)]
pub struct Spinner {
	pub(crate) raw: *mut lv_obj_t
}

impl Spinner {
	pub fn new(parent: Option<obj_mut>, cycle_time: Duration, arc_length: u32) -> Self {
		let parent = parent.map_or(ptr::null_mut(), |p| p.raw);
		Self {
			raw: unsafe { lv_spinner_create(
				parent,
				cycle_time.as_millis().try_into().expect("Spinner cycle time too long"),
				arc_length
			) },
		}
	}
}

unsafe impl Widget for Spinner {
	fn class() -> *const lv_obj_class_t {
		unsafe { &lvgl_sys::lv_spinner_class }
	}
}

impl Drop for Spinner {
	fn drop(&mut self) {
		trace!("spinner dropped");
		//unsafe { lv_obj_del(self.raw) }
	}
}
