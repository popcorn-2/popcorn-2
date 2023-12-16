use core::marker::PhantomData;
use log::trace;
use lvgl_sys::{lv_group_add_obj, lv_group_create, lv_group_del, lv_group_t};

use crate::input::Input;
use crate::object::obj_mut;

pub struct Group {
	pub(crate) raw: *mut lv_group_t
}

impl Group {
	pub fn new() -> Self {
		let raw = unsafe { lv_group_create() };
		Self {
			raw
		}
	}

	pub fn add_object(&mut self, object: obj_mut) {
		unsafe {
			lv_group_add_obj(self.raw, object.raw);
		}
	}
}

impl Drop for Group {
	fn drop(&mut self) {
		trace!("group dropped");
		//unsafe { lv_group_del(self.raw); }
	}
}
