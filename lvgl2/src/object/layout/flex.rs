use bitflags::bitflags;
use core::ffi::c_uint;

bitflags! {
    pub struct Align: c_uint {
        const START = lvgl_sys::lv_flex_align_t_LV_FLEX_ALIGN_START;
        const CENTER = lvgl_sys::lv_flex_align_t_LV_FLEX_ALIGN_CENTER;
        const END = lvgl_sys::lv_flex_align_t_LV_FLEX_ALIGN_END;
        const SPACE_AROUND = lvgl_sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_AROUND;
        const SPACE_BETWEEN = lvgl_sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_BETWEEN;
        const SPACE_EVENLY = lvgl_sys::lv_flex_align_t_LV_FLEX_ALIGN_SPACE_EVENLY;
    }
}

impl From<Align> for c_uint {
	fn from(value: Align) -> Self {
		value.bits() as c_uint
	}
}

bitflags! {
    pub struct Flow: c_uint {
        const COLUMN = lvgl_sys::lv_flex_flow_t_LV_FLEX_FLOW_COLUMN;
        const COLUMN_REVERSE = lvgl_sys::lv_flex_flow_t_LV_FLEX_FLOW_COLUMN_REVERSE;
        const COLUMN_WRAP = lvgl_sys::lv_flex_flow_t_LV_FLEX_FLOW_COLUMN_WRAP;
        const COLUMN_WRAP_REVERSE = lvgl_sys::lv_flex_flow_t_LV_FLEX_FLOW_COLUMN_WRAP_REVERSE;
        const ROW = lvgl_sys::lv_flex_flow_t_LV_FLEX_FLOW_ROW;
        const ROW_REVERSE = lvgl_sys::lv_flex_flow_t_LV_FLEX_FLOW_ROW_REVERSE;
        const ROW_WRAP = lvgl_sys::lv_flex_flow_t_LV_FLEX_FLOW_ROW_WRAP;
        const ROW_WRAP_REVERSE = lvgl_sys::lv_flex_flow_t_LV_FLEX_FLOW_ROW_WRAP_REVERSE;
    }
}

impl From<Flow> for c_uint {
	fn from(value: Flow) -> Self {
		value.bits() as c_uint
	}
}