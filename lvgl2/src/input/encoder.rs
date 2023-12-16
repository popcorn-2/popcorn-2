use alloc::boxed::Box;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;
use lvgl_sys::{lv_coord_t, lv_indev_data_t, lv_indev_drv_init, lv_indev_drv_register, lv_indev_drv_t, lv_indev_set_cursor, lv_indev_state_t_LV_INDEV_STATE_PRESSED, lv_indev_state_t_LV_INDEV_STATE_RELEASED, lv_indev_t, lv_indev_type_t_LV_INDEV_TYPE_ENCODER, lv_indev_type_t_LV_INDEV_TYPE_POINTER, LV_KEY_ENTER, LV_KEY_LEFT, LV_KEY_RIGHT};

use crate::object::obj_mut;

pub struct Driver<'cb> {
	raw: lv_indev_drv_t,
	_phantom: PhantomData<&'cb mut u8> // FIXME: What type to use here?
}

impl<'cb> Driver<'cb> {
	fn new_raw() -> lv_indev_drv_t {
		let mut d = MaybeUninit::uninit();
		unsafe { lv_indev_drv_init(d.as_mut_ptr()); }
		// SAFETY: `data` has been initialised by call to `lv_indev_drv_init`
		unsafe { d.assume_init() }
	}

	pub fn new_buttons<F: FnMut() -> ButtonUpdate + 'cb>(update: F) -> Self {
		let mut raw = Self::new_raw();

		raw.user_data = Box::into_raw(Box::new(update)).cast();
		raw.read_cb = Some(Self::update_trampoline_button::<F>);
		raw.type_ = lv_indev_type_t_LV_INDEV_TYPE_ENCODER;

		Self {
			raw,
			_phantom: PhantomData,
		}
	}
	pub fn new<F: FnMut() -> RotateUpdate + 'cb>(update: F) -> Self {
		let mut raw = Self::new_raw();

		raw.user_data = Box::into_raw(Box::new(update)).cast();
		raw.read_cb = Some(Self::update_trampoline_rotate::<F>);
		raw.type_ = lv_indev_type_t_LV_INDEV_TYPE_ENCODER;

		Self {
			raw,
			_phantom: PhantomData,
		}
	}

	unsafe extern "C" fn update_trampoline_rotate<F: FnMut() -> RotateUpdate>(
		driver: *mut lv_indev_drv_t,
		data: *mut lv_indev_data_t
	) {
		let f: &mut F = unsafe {
			// SAFETY:
			// `f` came from a Box of type F so valid size and alignment
			// Lifetime is upheld by `'f` lifetime parameter
			&mut *(*driver).user_data.cast::<F>()
		};
		let update = f();

		(*data).enc_diff = update.steps;
		(*data).state = if update.pressed { lv_indev_state_t_LV_INDEV_STATE_PRESSED } else { lv_indev_state_t_LV_INDEV_STATE_RELEASED };
	}

	unsafe extern "C" fn update_trampoline_button<F: FnMut() -> ButtonUpdate>(
		driver: *mut lv_indev_drv_t,
		data: *mut lv_indev_data_t
	) {
		let f: &mut F = unsafe {
			// SAFETY:
			// `f` came from a Box of type F so valid size and alignment
			// Lifetime is upheld by `'f` lifetime parameter
			&mut *(*driver).user_data.cast::<F>()
		};
		let update = f();

		(*data).key = match update {
			ButtonUpdate::Left => LV_KEY_LEFT,
			ButtonUpdate::Right => LV_KEY_RIGHT,
			ButtonUpdate::Click => LV_KEY_ENTER,
			_ => LV_KEY_ENTER
		};

		(*data).state = if update != ButtonUpdate::Released { lv_indev_state_t_LV_INDEV_STATE_PRESSED } else { lv_indev_state_t_LV_INDEV_STATE_RELEASED };
	}
}

impl<'cb> super::Driver for Driver<'cb> {
	unsafe fn register(&mut self) -> *mut lv_indev_t {
		lv_indev_drv_register(&mut self.raw)
	}
}

#[derive(Eq, PartialEq)]
pub enum ButtonUpdate {
	Left,
	Right,
	Click,
	Released
}

pub struct RotateUpdate {
	steps: i16,
	pressed: bool
}
