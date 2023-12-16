pub mod flex;

pub struct Layout {
	inner: u16,
}

impl Layout {
	/// Generates an `LV_LAYOUT_FLEX`
	pub fn flex() -> Self {
		Self {
			inner: unsafe { lvgl_sys::LV_LAYOUT_FLEX },
		}
	}

	/// Generates an `LV_LAYOUT_GRID`
	pub fn grid() -> Self {
		Self {
			inner: unsafe { lvgl_sys::LV_LAYOUT_GRID },
		}
	}
}

impl From<Layout> for u16 {
	fn from(value: Layout) -> Self {
		value.inner
	}
}
