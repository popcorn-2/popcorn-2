use core::borrow::Borrow;
use core::marker::PhantomData;
use core::mem;
use core::ptr::addr_of_mut;
use log::trace;

use crate::object::{obj, obj_mut, Object};

pub mod driver;
pub mod buffer;

pub struct Display<'driver, 'update_cb, 'drawbuf> {
	raw: *mut lvgl_sys::lv_disp_t,
	_phantom_driver: PhantomData<&'driver mut driver::Driver<'update_cb, 'drawbuf>>
}

impl<'driver, 'update_cb, 'drawbuf> Display<'driver, 'update_cb, 'drawbuf> {
	pub fn new(driver: &'driver mut driver::Driver<'update_cb, 'drawbuf>) -> Display<'driver, 'update_cb, 'drawbuf> {
		// SAFETY: driver address is valid until driver is dropped, which can't happen until Self is dropped, at which point driver is unregistered
		let raw = unsafe { lvgl_sys::lv_disp_drv_register(addr_of_mut!(driver.raw)) };
		if raw.is_null() { todo!("error") }

		Display {
			raw,
			_phantom_driver: PhantomData
		}
	}

	pub fn active_screen(&mut self) -> obj_mut<'_> {
		let raw = unsafe { lvgl_sys::lv_disp_get_scr_act(self.raw) };
		obj_mut {
			raw,
			_phantom: PhantomData,
		}
	}

	pub fn swap_screen(&mut self, screen: impl Into<Object>) -> Object {
		let screen = screen.into();

		let old_raw = unsafe { lvgl_sys::lv_disp_get_scr_act(self.raw) };

		unsafe { lvgl_sys::lv_disp_load_scr(screen.raw); }
		mem::forget(screen);

		Object {
			raw: old_raw
		}
	}
}

impl Drop for Display<'_, '_, '_> {
	fn drop(&mut self) {
		trace!("Display dropped");
		unsafe { lvgl_sys::lv_disp_remove(self.raw) };
	}
}
