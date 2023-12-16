use alloc::boxed::Box;
use core::marker::PhantomData;
use core::ptr;
use log::trace;
use lvgl_sys::{lv_btn_create, lv_obj_class_t, lv_obj_del, lv_obj_t};

use crate::object::{obj_mut, Widget};

#[repr(transparent)]
pub struct Button<'f> {
	pub(crate) raw: *mut lv_obj_t,
	_phantom_cb: PhantomData<&'f mut u8>
}

impl Button<'_> {
	pub fn new_with_callback<'f, F: FnMut() + 'f>(parent: Option<obj_mut>, callback: F) -> Button<'f> {
		let parent = parent.map_or(ptr::null_mut(), |p| p.raw);
		let raw = unsafe { lv_btn_create(parent) };

		// FIXME: memory leak here
		unsafe { lvgl_sys::lv_obj_add_event_cb(raw, Some(Self::callback_trampoline::<F>), lvgl_sys::lv_event_code_t_LV_EVENT_PRESSED, Box::into_raw(Box::new(callback)).cast()); }

		Button {
			raw,
			_phantom_cb: PhantomData,
		}
	}

	pub fn new(parent: Option<obj_mut>) -> Button<'static> {
		let parent = parent.map_or(ptr::null_mut(), |p| p.raw);
		let raw = unsafe { lv_btn_create(parent) };

		Button {
			raw,
			_phantom_cb: PhantomData,
		}
	}

	unsafe extern "C" fn callback_trampoline<F>(event: *mut lvgl_sys::lv_event_t) where F: FnMut() {
		let f: &mut F = unsafe {
			// SAFETY:
			// `f` came from a Box of type F so valid size and alignment
			// Lifetime is upheld by `'f` lifetime parameter
			&mut *(*event).user_data.cast::<F>()
		};
		f();
	}
}

unsafe impl Widget for Button<'_> {
	fn class() -> *const lv_obj_class_t {
		unsafe { &lvgl_sys::lv_btn_class }
	}
}

impl Drop for Button<'_> {
	fn drop(&mut self) {
		trace!("button dropped");
		//unsafe { lv_obj_del(self.raw); }
	}
}
