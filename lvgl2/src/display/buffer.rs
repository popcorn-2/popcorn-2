use alloc::boxed::Box;
use core::mem::MaybeUninit;
use core::ptr;

use crate::misc::Color;

pub struct DrawBuffer {
	pub(crate) raw: lvgl_sys::lv_disp_draw_buf_t,
	first_buffer: Box<[Color]>,
	second_buffer: Box<[Color]>,
}

impl DrawBuffer {
	pub fn new(size: u32) -> Self {
		let mut raw = MaybeUninit::uninit();
		let mut first_buffer = Box::<[Color]>::new_uninit_slice(size as usize);
		unsafe {
			lvgl_sys::lv_disp_draw_buf_init(raw.as_mut_ptr(), first_buffer.as_mut_ptr().cast(), ptr::null_mut(), size);

			Self {
				raw: raw.assume_init(),
				first_buffer: first_buffer.assume_init(),
				second_buffer: Box::new([])
			}
		}
	}

	pub fn new_double(size: u32) -> Self {
		let mut raw = MaybeUninit::uninit();
		let mut first_buffer = Box::<[Color]>::new_uninit_slice(size as usize);
		let mut second_buffer = Box::<[Color]>::new_uninit_slice(size as usize);
		unsafe {
			lvgl_sys::lv_disp_draw_buf_init(raw.as_mut_ptr(), first_buffer.as_mut_ptr().cast(), second_buffer.as_mut_ptr().cast(), size);

			Self {
				raw: raw.assume_init(),
				first_buffer: first_buffer.assume_init(),
				second_buffer: second_buffer.assume_init()
			}
		}
	}
}
