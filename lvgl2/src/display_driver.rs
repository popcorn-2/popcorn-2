use alloc::boxed::Box;
use bitflags::{bitflags, Flags};
use core::{mem, ptr};
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::{addr_of_mut, slice_from_raw_parts};
use lvgl_sys::lv_obj_t;
use paste::paste;

use crate::draw_buffer::DrawBuffer;
use crate::Error;

impl<'draw_buf> DisplayDriver<'draw_buf> {
	pub fn new<F: FnMut(DisplayUpdate) + 'a>(
		draw_buffer: &'a mut DrawBuffer,
		display_width: lvgl_sys::lv_coord_t,
		display_height: lvgl_sys::lv_coord_t,
		flush_callback: F
	) -> Self {
		let mut driver = MaybeUninit::uninit();

		// SAFETY: `data` is correct size?
		unsafe { lvgl_sys::lv_disp_drv_init(driver.as_mut_ptr()); }

		// SAFETY: `data` has been initialised by call to `lv_disp_drv_init`
		let mut driver = unsafe { driver.assume_init() };

		driver.draw_buf = addr_of_mut!(draw_buffer.data);
		driver.user_data = Box::into_raw(Box::new(flush_callback)).cast();
		driver.flush_cb = Some(Self::flush_trampoline::<F>);
		driver.hor_res = display_width;
		driver.ver_res = display_height;

		Self {
			driver,
			_phantom: PhantomData,
		}
	}

	unsafe extern "C" fn flush_trampoline<F: FnMut(DisplayUpdate)>(
		display_driver: *mut lvgl_sys::lv_disp_drv_t,
		area: *const lvgl_sys::lv_area_t,
		colors: *mut lvgl_sys::lv_color_t
	) {
		let area = Area::from_raw(area);
		// TODO: is this the right length?
		let colors = slice_from_raw_parts(colors, (*(*display_driver).draw_buf).size as usize);
		let update = DisplayUpdate {
			area,
			colors: unsafe { mem::transmute(&*colors) }
		};
		let f: &mut F = unsafe {
			// SAFETY:
			// `f` came from a Box of type F so valid size and alignment
			// Lifetime is upheld by `'f` lifetime parameter
			&mut *(*display_driver).user_data.cast::<F>()
		};
		f(update)
	}
}

pub struct DisplayUpdate<'a> {
	pub area: Area,
	pub colors: &'a [Color]
}

pub struct Display<'driver, 'a> {
	display: *mut lvgl_sys::lv_disp_t,
	_phantom_display_driver: PhantomData<&'driver mut DisplayDriver<'a>>
}

impl<'driver, 'a> Display<'driver, 'a> {
	pub fn new(driver: &'driver mut DisplayDriver) -> Result<Self, Error> {
		// SAFETY: `driver` is mutably borrowed for at least 'self, and is unregistered on drop of Self
		let display = unsafe { lvgl_sys::lv_disp_drv_register(addr_of_mut!(driver.driver)) };
		if display.is_null() { return Err(Error); }

		Ok(Self {
			display,
			_phantom_display_driver: PhantomData
		})
	}

	pub fn update_driver<'new_driver, 'b>(self, new_driver: &'new_driver mut DisplayDriver<'b>) -> Display<'new_driver, 'b> {
		// SAFETY:
		// updated Display only contains references to new driver so all references to existing driver are removed and lifetime can therefore be dropped
		unsafe {
			lvgl_sys::lv_disp_drv_update(self.display, addr_of_mut!(new_driver.driver));
		}

		Display {
			display: self.display,
			_phantom_display_driver: PhantomData
		}
	}

	// FIXME: This operates on default screen not 'self'
	pub fn load_screen(&mut self, screen: &mut Screen) {
		unsafe { lvgl_sys::lv_disp_load_scr(screen.raw) };
	}

	pub fn active_screen(&self) -> Screen {
		let raw = unsafe { lvgl_sys::lv_disp_get_scr_act(self.display) };

		// FIXME: This screen needs to be somehow invalidated on Drop
		Screen { raw }
	}
}

impl<'a, 'b> Drop for Display<'a, 'b> {
	fn drop(&mut self) {
		unsafe { lvgl_sys::lv_disp_remove(self.display); }
	}
}

pub struct Screen {
	raw: *mut lvgl_sys::lv_obj_t
}

impl Screen {
	pub fn new() -> Result<Self, Error> {
		let raw = unsafe { lvgl_sys::lv_obj_create(ptr::null_mut()) };
		if raw.is_null() { return Err(Error); }

		Ok(Self { raw })
	}
}

impl Widget for Screen {
	fn raw(&mut self) -> *mut lv_obj_t { self.raw }
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
	pub struct State: u32 {
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

pub trait Widget {
	fn raw(&mut self) -> *mut lvgl_sys::lv_obj_t;

	fn inline_style(&mut self, part: Part, state: State) -> InlineStyle<'_, Self> where Self: Sized {
		InlineStyle {
			widget: self,
			selector: lvgl_sys::lv_part_t::from(part) | u32::from(lvgl_sys::lv_state_t::from(state))
		}
	}
}

pub struct InlineStyle<'a, T: Widget> {
	widget: &'a mut T,
	selector: lvgl_sys::lv_style_selector_t
}

pub struct ExternalStyle {
	raw: lvgl_sys::lv_style_t
}

impl ExternalStyle {
	pub fn new() -> Self {
		let mut raw = MaybeUninit::uninit();

		unsafe {
			lvgl_sys::lv_style_init(raw.as_mut_ptr());
		}

		let raw = unsafe { raw.assume_init() };
		Self { raw }
	}
}

macro gen_lv_style_trait {
	($($property:ident, $vty:ty),*) => {
		$(paste! {
			fn [<set_ $property>](&mut self, value: $vty);
		})*
	}
}

macro gen_lv_style_inline($property:ident, $vty:ty) {
	paste! {
		#[inline]
		fn [<set_ $property>](&mut self, value: $vty) {
			unsafe {
                lvgl_sys::[<lv_obj_set_style_ $property>](
                    self.widget.raw(),
                    value.into(),
	                self.selector
                );
            }
		}
	}
}

bitflags! {
    pub struct Opacity: u32 {
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

pub trait Style {
	fn set_align(&mut self, value: u8);
	fn set_bg_color(&mut self, value: Color);
	fn set_bg_opa(&mut self, value: Opacity);
}

impl<T: Widget> Style for InlineStyle<'_, T> {
	gen_lv_style_inline!(align, u8);
	gen_lv_style_inline!(bg_color, Color);
	gen_lv_style_inline!(bg_opa, Opacity);
}
