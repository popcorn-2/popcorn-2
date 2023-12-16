use core::marker::PhantomData;
use core::mem;
use log::trace;
use lvgl_sys::{lv_indev_delete, lv_indev_set_cursor, lv_indev_set_group, lv_indev_t};

use crate::object::group::Group;
use crate::object::obj_mut;

pub mod pointer;
pub mod encoder;

pub struct Input<'driver> {
	raw: *mut lv_indev_t,
	_phantom: PhantomData<&'driver mut dyn Driver>
}

impl<'driver> Input<'driver> {
	pub fn new(driver: &'driver mut impl Driver) -> Self {
		let raw = unsafe { driver.register() };
		Self {
			raw,
			_phantom: PhantomData
		}
	}

	pub fn set_cursor(&mut self, cursor: obj_mut) {
		unsafe {
			lv_indev_set_cursor(self.raw, cursor.raw);
		}
	}

	pub fn set_group(&mut self, group: Group) {
		unsafe {
			lv_indev_set_group(self.raw, group.raw);
		}
		mem::forget(group);
	}
}

impl Drop for Input<'_> {
	fn drop(&mut self) {
		trace!("Input device dropped");
		unsafe {
			lv_indev_delete(self.raw);
		}
	}
}

pub trait Driver {
	unsafe fn register(&mut self) -> *mut lv_indev_t;
}
