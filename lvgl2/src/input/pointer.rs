use alloc::boxed::Box;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;
use lvgl_sys::{lv_coord_t, lv_indev_data_t, lv_indev_drv_init, lv_indev_drv_register, lv_indev_drv_t, lv_indev_set_cursor, lv_indev_state_t_LV_INDEV_STATE_PRESSED, lv_indev_state_t_LV_INDEV_STATE_RELEASED, lv_indev_t, lv_indev_type_t_LV_INDEV_TYPE_POINTER};

use crate::object::obj_mut;

pub struct Driver<'cb> {
	raw: lv_indev_drv_t,
	_phantom: PhantomData<&'cb mut u8> // FIXME: What type to use here?
}

impl<'cb> Driver<'cb> {
	pub fn new<F: FnMut() -> Update + 'cb>(update: F) -> Self {
		let mut raw = {
			let mut d = MaybeUninit::uninit();
			unsafe { lv_indev_drv_init(d.as_mut_ptr()); }
			// SAFETY: `data` has been initialised by call to `lv_indev_drv_init`
			unsafe { d.assume_init() }
		};

		raw.user_data = Box::into_raw(Box::new(update)).cast();
		raw.read_cb = Some(Self::update_trampoline::<F>);
		raw.type_ = lv_indev_type_t_LV_INDEV_TYPE_POINTER;

		Self {
			raw,
			_phantom: PhantomData,
		}
	}

	unsafe extern "C" fn update_trampoline<F: FnMut() -> Update>(
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

		(*data).point.x = update.location.0;
		(*data).point.y = update.location.1;
		(*data).state = if update.pressed { lv_indev_state_t_LV_INDEV_STATE_PRESSED } else { lv_indev_state_t_LV_INDEV_STATE_RELEASED };
	}
}

impl<'cb> super::Driver for Driver<'cb> {
	unsafe fn register(&mut self) -> *mut lv_indev_t {
		lv_indev_drv_register(&mut self.raw)
	}
}

pub struct Update {
	pub pressed: bool,
	pub location: (lv_coord_t, lv_coord_t)
}
