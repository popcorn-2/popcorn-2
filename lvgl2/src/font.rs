use lvgl_sys::lv_font_t;

#[derive(Copy, Clone, Debug)]
pub struct Font<'a> {
	inner: &'a lv_font_t
}

impl Font<'_> {
	pub fn new(font: &lv_font_t) -> Font<'_> {
		Font {
			inner: font
		}
	}
}

impl From<Font<'_>> for *const lv_font_t {
	fn from(value: Font<'_>) -> Self {
		value.inner
	}
}
