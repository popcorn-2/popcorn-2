use core::fmt::{Debug, Formatter};

#[derive(Debug)]
pub struct Area {
	pub x1: lvgl_sys::lv_coord_t,
	pub y1: lvgl_sys::lv_coord_t,
	pub x2: lvgl_sys::lv_coord_t,
	pub y2: lvgl_sys::lv_coord_t,
}

impl Area {
	pub unsafe fn from_raw(area: *const lvgl_sys::lv_area_t) -> Self {
		Self {
			x1: (*area).x1,
			y1: (*area).y1,
			x2: (*area).x2,
			y2: (*area).y2,
		}
	}
}


#[repr(transparent)]
pub struct Color(lvgl_sys::lv_color_t);

impl From<Color> for lvgl_sys::lv_color_t {
	fn from(value: Color) -> Self {
		value.0
	}
}

impl Color {
	pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
		Self(
			unsafe { lvgl_sys::_LV_COLOR_MAKE(r, g, b) }
		)
	}

	pub fn r(&self) -> u8 {
		unsafe { self.0.ch.red }
	}

	pub fn g(&self) -> u8 {
		unsafe { self.0.ch.green }
	}

	pub fn b(&self) -> u8 {
		unsafe { self.0.ch.blue }
	}

	pub fn a(&self) -> u8 {
		unsafe { self.0.ch.alpha }
	}

	pub fn set_r(&mut self, val: u8) {
		unsafe { self.0.ch.red = val; }
	}

	pub fn set_g(&mut self, val: u8) {
		unsafe { self.0.ch.green = val; }
	}

	pub fn set_b(&mut self, val: u8) {
		unsafe { self.0.ch.blue = val; }
	}

	pub fn set_a(&mut self, val: u8) {
		unsafe { self.0.ch.alpha = val; }
	}
}

impl Debug for Color {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Color")
				.field("r", &self.r())
				.field("g", &self.g())
				.field("b", &self.b())
				.field("a", &self.a())
				.finish()
	}
}

pub static LV_SIZE_CONTENT: u32 = 2001 | lvgl_sys::_LV_COORD_TYPE_SPEC;

pub fn pct(pct: lvgl_sys::lv_coord_t) -> lvgl_sys::lv_coord_t {
	if pct > 0 {
		pct | unsafe {
			<u32 as TryInto<lvgl_sys::lv_coord_t>>::try_into(lvgl_sys::_LV_COORD_TYPE_SPEC).unwrap_unchecked()
		}
	} else {
		(1000 - pct)
				| unsafe {
			<u32 as TryInto<lvgl_sys::lv_coord_t>>::try_into(lvgl_sys::_LV_COORD_TYPE_SPEC)
					.unwrap_unchecked()
		}
	}
}
