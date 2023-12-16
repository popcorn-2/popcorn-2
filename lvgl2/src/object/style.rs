use bitflags::bitflags;
use core::ffi::c_uint;
use core::mem::MaybeUninit;
use lvgl_sys::{lv_coord_t, lv_style_init, lv_style_selector_t, lv_style_t};
use paste::paste;

use crate::font::Font;
use crate::misc::Color;
use crate::object::{obj, obj_mut};
use crate::object::layout::{flex, Layout};

pub trait Style {
	gen_lv_style_trait!(set_bg_color, Color);
	gen_lv_style_trait!(set_bg_opa, Opacity);
	gen_lv_style_trait!(set_text_color, Color);
	gen_lv_style_trait!(set_border_width, lv_coord_t);
	gen_lv_style_trait!(set_radius, lv_coord_t);
	gen_lv_style_trait!(set_text_font, Font);
	gen_lv_style_trait!(set_layout, Layout);
	gen_lv_style_trait!(set_flex_flow, flex::Flow);
	gen_lv_style_trait!(set_flex_main_place, flex::Align);
	gen_lv_style_trait!(set_flex_cross_place, flex::Align);
	gen_lv_style_trait!(set_flex_track_place, flex::Align);
	gen_lv_style_trait!(set_pad_row, lv_coord_t);
	gen_lv_style_trait!(set_pad_column, lv_coord_t);
	gen_lv_style_trait!(set_pad_top, lv_coord_t);
	gen_lv_style_trait!(set_pad_bottom, lv_coord_t);
	gen_lv_style_trait!(set_pad_left, lv_coord_t);
	gen_lv_style_trait!(set_pad_right, lv_coord_t);
	gen_lv_style_trait!(set_width, lv_coord_t);
	gen_lv_style_trait!(set_height, lv_coord_t);
	fn set_size(&mut self, width: lv_coord_t, height: lv_coord_t);
	gen_lv_style_trait!(set_align, Align);
	gen_lv_style_trait!(set_outline_color, Color);
	gen_lv_style_trait!(set_outline_pad, lv_coord_t);
	gen_lv_style_trait!(set_outline_width, lv_coord_t);
}

bitflags! {
    pub struct Opacity: c_uint {
        const OPA_TRANSP = lvgl_sys::LV_OPA_TRANSP;
        const OPA_0 = lvgl_sys::LV_OPA_0;
        const OPA_10 = lvgl_sys::LV_OPA_10;
        const OPA_20 = lvgl_sys::LV_OPA_20;
        const OPA_30 = lvgl_sys::LV_OPA_30;
        const OPA_40 = lvgl_sys::LV_OPA_40;
        const OPA_50 = lvgl_sys::LV_OPA_50;
        const OPA_60 = lvgl_sys::LV_OPA_60;
        const OPA_70 = lvgl_sys::LV_OPA_70;
        const OPA_80 = lvgl_sys::LV_OPA_80;
        const OPA_90 = lvgl_sys::LV_OPA_90;
        const OPA_100 = lvgl_sys::LV_OPA_100;
        const OPA_COVER = lvgl_sys::LV_OPA_COVER;
    }
}

impl From<Opacity> for u8 {
	fn from(value: Opacity) -> Self {
		value.bits() as _
	}
}


pub enum Part {
	Main,
	Scrollbar,
	Indicator,
	Knob,
	Selected,
	Items,
	Ticks,
	Cursor,
	CustomFirst,
	Any,
}

impl Default for Part {
	fn default() -> Self {
		Self::Main
	}
}

impl From<Part> for lvgl_sys::lv_part_t {
	fn from(self_: Part) -> lvgl_sys::lv_part_t {
		match self_ {
			Part::Main => lvgl_sys::LV_PART_MAIN,
			Part::Scrollbar => lvgl_sys::LV_PART_SCROLLBAR,
			Part::Indicator => lvgl_sys::LV_PART_INDICATOR,
			Part::Knob => lvgl_sys::LV_PART_KNOB,
			Part::Selected => lvgl_sys::LV_PART_SELECTED,
			Part::Items => lvgl_sys::LV_PART_ITEMS,
			Part::Ticks => lvgl_sys::LV_PART_TICKS,
			Part::Cursor => lvgl_sys::LV_PART_CURSOR,
			Part::CustomFirst => lvgl_sys::LV_PART_CUSTOM_FIRST,
			Part::Any => lvgl_sys::LV_PART_ANY,
		}
	}
}

bitflags! {
	pub struct State: c_uint {
		const DEFAULT = lvgl_sys::LV_STATE_DEFAULT;
		const CHECKED = lvgl_sys::LV_STATE_CHECKED;
		const FOCUSED = lvgl_sys::LV_STATE_FOCUSED;
		const FOCUS_KEY = lvgl_sys::LV_STATE_FOCUS_KEY;
		const EDITED = lvgl_sys::LV_STATE_EDITED;
		const HOVERED = lvgl_sys::LV_STATE_HOVERED;
		const PRESSED = lvgl_sys::LV_STATE_PRESSED;
		const SCROLLED = lvgl_sys::LV_STATE_SCROLLED;
		const DISABLED = lvgl_sys::LV_STATE_DISABLED;
		const ANY = lvgl_sys::LV_STATE_ANY;
	}
}

impl From<State> for lvgl_sys::lv_state_t {
	fn from(value: State) -> Self {
		value.bits() as u16
	}
}

macro gen_lv_style_trait($property:ident, $vty:ty) {
	fn $property (&mut self, value: $vty);
}

macro gen_lv_style_inline($property:ident, $vty:ty) {
	paste! {
		#[inline]
		fn [<set_ $property>](&mut self, value: $vty) {
			unsafe {
                lvgl_sys::[<lv_obj_set_style_ $property>](
                    self.widget.raw,
                    value.into(),
	                self.selector
                );
            }
		}
	}
}

macro gen_lv_style_external($property:ident, $vty:ty) {
paste! {
		#[inline]
		fn [<set_ $property>](&mut self, value: $vty) {
			unsafe {
                lvgl_sys::[<lv_style_set_ $property>](
                    &mut self.raw,
                    value.into()
                );
            }
		}
	}
}

pub struct InlineStyle<'a> {
	pub(crate) widget: obj_mut<'a>,
	pub(crate) selector: lv_style_selector_t
}

impl Style for InlineStyle<'_> {
	gen_lv_style_inline!(bg_color, Color);
	gen_lv_style_inline!(bg_opa, Opacity);
	gen_lv_style_inline!(text_color, Color);
	gen_lv_style_inline!(border_width, lv_coord_t);
	gen_lv_style_inline!(radius, lv_coord_t);
	gen_lv_style_inline!(text_font, Font);
	gen_lv_style_inline!(layout, Layout);
	gen_lv_style_inline!(flex_flow, flex::Flow);
	gen_lv_style_inline!(flex_main_place, flex::Align);
	gen_lv_style_inline!(flex_cross_place, flex::Align);
	gen_lv_style_inline!(flex_track_place, flex::Align);
	gen_lv_style_inline!(pad_row, lv_coord_t);
	gen_lv_style_inline!(pad_column, lv_coord_t);
	gen_lv_style_inline!(pad_top, lv_coord_t);
	gen_lv_style_inline!(pad_bottom, lv_coord_t);
	gen_lv_style_inline!(pad_left, lv_coord_t);
	gen_lv_style_inline!(pad_right, lv_coord_t);
	gen_lv_style_inline!(width, lv_coord_t);
	gen_lv_style_inline!(height, lv_coord_t);

	#[inline]
	fn set_size(&mut self, width: lv_coord_t, height: lv_coord_t) {
		unsafe {
			lvgl_sys::lv_obj_set_size(
				self.widget.raw,
				width,
				height
			);
		}
	}

	gen_lv_style_inline!(align, Align);
	gen_lv_style_inline!(outline_color, Color);
	gen_lv_style_inline!(outline_pad, lv_coord_t);
	gen_lv_style_inline!(outline_width, lv_coord_t);
}

pub struct ExternalStyle {
	pub(crate) raw: lv_style_t
}

impl ExternalStyle {
	pub fn new() -> Self {
		let mut raw = MaybeUninit::<lv_style_t>::uninit();
		unsafe { lv_style_init(raw.as_mut_ptr()); }
		Self {
			raw: unsafe { raw.assume_init() }
		}
	}
}

impl Style for ExternalStyle {
	gen_lv_style_external!(bg_color, Color);
	gen_lv_style_external!(bg_opa, Opacity);
	gen_lv_style_external!(text_color, Color);
	gen_lv_style_external!(border_width, lv_coord_t);
	gen_lv_style_external!(radius, lv_coord_t);
	gen_lv_style_external!(text_font, Font);
	gen_lv_style_external!(layout, Layout);
	gen_lv_style_external!(flex_flow, flex::Flow);
	gen_lv_style_external!(flex_main_place, flex::Align);
	gen_lv_style_external!(flex_cross_place, flex::Align);
	gen_lv_style_external!(flex_track_place, flex::Align);
	gen_lv_style_external!(pad_row, lv_coord_t);
	gen_lv_style_external!(pad_column, lv_coord_t);
	gen_lv_style_external!(pad_top, lv_coord_t);
	gen_lv_style_external!(pad_bottom, lv_coord_t);
	gen_lv_style_external!(pad_left, lv_coord_t);
	gen_lv_style_external!(pad_right, lv_coord_t);
	gen_lv_style_external!(width, lv_coord_t);
	gen_lv_style_external!(height, lv_coord_t);

	#[inline]
	fn set_size(&mut self, _: lv_coord_t, _: lv_coord_t) {
		unimplemented!("Cannot set size through external style")
	}

	gen_lv_style_external!(align, Align);
	gen_lv_style_external!(outline_color, Color);
	gen_lv_style_external!(outline_pad, lv_coord_t);
	gen_lv_style_external!(outline_width, lv_coord_t);
}

pub enum Align {
	Center,
	TopLeft,
	TopMid,
	TopRight,
	BottomLeft,
	BottomMid,
	BottomRight,
	LeftMid,
	RightMid,
	OutTopLeft,
	OutTopMid,
	OutTopRight,
	OutBottomLeft,
	OutBottomMid,
	OutBottomRight,
	OutLeftTop,
	OutLeftMid,
	OutLeftBottom,
	OutRightTop,
	OutRightMid,
	OutRightBottom,
}

impl From<Align> for u8 {
	fn from(value: Align) -> u8 {
		let native = match value {
			Align::Center => lvgl_sys::LV_ALIGN_CENTER,
			Align::TopLeft => lvgl_sys::LV_ALIGN_TOP_LEFT,
			Align::TopMid => lvgl_sys::LV_ALIGN_TOP_MID,
			Align::TopRight => lvgl_sys::LV_ALIGN_TOP_RIGHT,
			Align::BottomLeft => lvgl_sys::LV_ALIGN_BOTTOM_LEFT,
			Align::BottomMid => lvgl_sys::LV_ALIGN_BOTTOM_MID,
			Align::BottomRight => lvgl_sys::LV_ALIGN_BOTTOM_RIGHT,
			Align::LeftMid => lvgl_sys::LV_ALIGN_LEFT_MID,
			Align::RightMid => lvgl_sys::LV_ALIGN_RIGHT_MID,
			Align::OutTopLeft => lvgl_sys::LV_ALIGN_OUT_TOP_LEFT,
			Align::OutTopMid => lvgl_sys::LV_ALIGN_OUT_TOP_MID,
			Align::OutTopRight => lvgl_sys::LV_ALIGN_OUT_TOP_RIGHT,
			Align::OutBottomLeft => lvgl_sys::LV_ALIGN_OUT_BOTTOM_LEFT,
			Align::OutBottomMid => lvgl_sys::LV_ALIGN_OUT_BOTTOM_MID,
			Align::OutBottomRight => lvgl_sys::LV_ALIGN_OUT_BOTTOM_RIGHT,
			Align::OutLeftTop => lvgl_sys::LV_ALIGN_OUT_LEFT_TOP,
			Align::OutLeftMid => lvgl_sys::LV_ALIGN_OUT_LEFT_MID,
			Align::OutLeftBottom => lvgl_sys::LV_ALIGN_OUT_LEFT_BOTTOM,
			Align::OutRightTop => lvgl_sys::LV_ALIGN_OUT_RIGHT_TOP,
			Align::OutRightMid => lvgl_sys::LV_ALIGN_OUT_RIGHT_MID,
			Align::OutRightBottom => lvgl_sys::LV_ALIGN_OUT_RIGHT_BOTTOM,
		};
		native as u8
	}
}
