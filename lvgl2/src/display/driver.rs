use alloc::boxed::Box;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::{addr_of_mut, slice_from_raw_parts, slice_from_raw_parts_mut};

use crate::misc::{Area, Color};

use super::buffer::DrawBuffer;

pub struct Driver<'update_cb, 'draw_buffer> {
	pub(crate) raw: lvgl_sys::lv_disp_drv_t,
	//draw_buffer: DrawBuffer,
	_phantom: PhantomData<&'update_cb mut u8>, // FIXME: What type to use here?
	_phantom_drawbuf: PhantomData<&'draw_buffer mut DrawBuffer>
}

impl<'update_cb, 'draw_buffer> Driver<'update_cb, 'draw_buffer> {
	pub fn new<F>(draw_buffer: &'draw_buffer mut DrawBuffer, display_width: usize, display_height: usize, update: F) -> Self
		where F: FnMut(DisplayUpdate) + 'update_cb
	{
		let mut raw = {
			let mut d = MaybeUninit::uninit();
			unsafe { lvgl_sys::lv_disp_drv_init(d.as_mut_ptr()); }
			// SAFETY: `data` has been initialised by call to `lv_disp_drv_init`
			unsafe { d.assume_init() }
		};

		raw.draw_buf = &mut draw_buffer.raw;
		raw.user_data = Box::into_raw(Box::new(update)).cast();
		raw.flush_cb = Some(Self::update_trampoline::<F>);
		raw.hor_res = display_width.try_into().expect("Display width too large for LVGL");
		raw.ver_res = display_height.try_into().expect("Display height too large for LVGL");

		Self {
			raw,
			_phantom: PhantomData,
			_phantom_drawbuf: PhantomData
		}
	}

	unsafe extern "C" fn update_trampoline<F: FnMut(DisplayUpdate)>(
		display_driver: *mut lvgl_sys::lv_disp_drv_t,
		area: *const lvgl_sys::lv_area_t,
		colors: *mut lvgl_sys::lv_color_t
	) {
		let area = Area::from_raw(area);
		// SAFETY: Color is transparent around lv_color_t
		let colors = colors.cast::<Color>();
		// TODO: is this the right length?
		let colors = slice_from_raw_parts_mut(colors, (*(*display_driver).draw_buf).size as usize);
		let update = DisplayUpdate {
			area,
			colors: unsafe { &mut *colors }
		};
		let f: &mut F = unsafe {
			// SAFETY:
			// `f` came from a Box of type F so valid size and alignment
			// Lifetime is upheld by `'f` lifetime parameter
			&mut *(*display_driver).user_data.cast::<F>()
		};
		f(update);

		unsafe { lvgl_sys::lv_disp_flush_ready(display_driver); }
	}
}

#[derive(Debug)]
pub struct DisplayUpdate<'a> {
	pub area: Area,
	pub colors: &'a mut [Color]
}
