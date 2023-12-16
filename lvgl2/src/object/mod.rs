/*use core::{mem, ptr};
use core::ptr::{addr_of_mut, from_raw_parts_mut};
use lvgl_sys::{lv_obj_class, lv_obj_class_t, lv_obj_create, lv_obj_t};
use log::debug;

/// # SAFETY
/// The object must be `repr(transparent)` around an `lv_obj_t`.
/// The class returned by [`Widget::class()`] must be the same class as the widget actually is.
/// On creation of the object, the [`user_data`](lvgl_sys::lv_obj_t::user_data) field must be initialed with the [`Widget`] vtable pointer.
pub unsafe trait Widget {
	fn class() -> *const lv_obj_class_t where Self: Sized;
	fn raw(&self) -> *mut lv_obj_t {
		unsafe { *(self as *const _ as *mut *mut lv_obj_t) }
	}
}

impl dyn Widget + '_ {
	pub unsafe fn new(raw: *mut lv_obj_t) -> *mut Self {
		let vtable = (*raw).user_data;
		from_raw_parts_mut::<Self>(raw.cast(), mem::transmute(vtable)) // Is this transmute safe?
	}

	pub fn downcast_ref<T: Widget>(&self) -> Option<&T> {
		let class = unsafe { (*self.raw()).class_p };
		debug!("actual class {:p}, match class {:p}", class, T::class());
		if T::class() == class {
			Some(unsafe { self.downcast_ref_unchecked() })
		} else { None }
	}

	pub fn downcast_mut<T: Widget>(&mut self) -> Option<&mut T> {
		let class = unsafe { (*self.raw()).class_p };
		debug!("actual class {:p}, match class {:p}", class, T::class());
		if T::class() == class {
			Some(unsafe { self.downcast_mut_unchecked() })
		} else { None }
	}

	pub unsafe fn downcast_ref_unchecked<T: Widget>(&self) -> &T {
		&*(self as *const dyn Widget as *const T)
	}

	pub unsafe fn downcast_mut_unchecked<T: Widget>(&mut self) -> &mut T {
		&mut *(self as *mut dyn Widget as *mut T)
	}
}

#[repr(transparent)]
pub struct Object {
	raw: *mut lv_obj_t
}

impl Object {
	pub fn new(parent: &mut dyn Widget) -> Self {
		Self::new_raw(parent.raw())
	}
	pub fn new_screen() -> Self {
		Self::new_raw(ptr::null_mut())
	}

	pub fn new_raw(parent: *mut lv_obj_t) -> Self {
		let raw = unsafe { lv_obj_create(parent) };
		let ret = Self {
			raw
		};
		let (_, vtable) = (&ret as &dyn Widget as *const dyn Widget).to_raw_parts();
		unsafe { (*raw).user_data = mem::transmute(vtable) }

		debug!("class_p for new object is {:p}, from compiler vtable {:p}, from custom vtable {:p}",
			unsafe { (*raw).class_p },
			unsafe { (*(&ret as &dyn Widget).raw()).class_p },
			unsafe { (*(*<dyn Widget>::new(raw)).raw()).class_p }
		);

		ret
	}

	pub(crate) fn set_vtable(raw: *mut lv_obj_t) {
		let obj = Self {
			raw
		};
		let (_, vtable) = (&obj as &dyn Widget as *const dyn Widget).to_raw_parts();
		unsafe { (*raw).user_data = mem::transmute(vtable) }
	}
}

unsafe impl Widget for Object {
	fn class() -> *const lv_obj_class_t {
		unsafe { &lv_obj_class }
	}

	fn raw(&self) -> *mut lv_obj_t {
		self.raw
	}
}

impl Drop for Object {
	fn drop(&mut self) {
		// TODO: If parented, do nothing since lvgl cleans up children. If unparented, and therefore a screen, only clean up if not currently loaded, since [`Display::drop`] cleans up the loaded screen
	}
}
*/

use core::{mem, ptr};
use core::borrow::Borrow;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use log::trace;
use lvgl_sys::{lv_obj_add_style, lv_obj_class_t, lv_obj_create, lv_obj_del, lv_obj_t, lv_part_t, lv_state_t};

use crate::object::style::{ExternalStyle, InlineStyle, Part, State};

pub mod style;
pub mod layout;
pub mod label;
pub mod button;
pub mod image;
pub mod group;
pub mod spinner;
pub mod textarea;

/// Represents an owned `lv_obj_t`
#[repr(transparent)]
pub struct Object {
	pub raw: *mut lv_obj_t
}

/// Represents a polymorphic borrowed `lv_obj_t*`
#[derive(Copy, Clone)]
#[repr(transparent)]
#[allow(non_camel_case_types)]
pub struct obj<'a> {
	pub raw: *const lv_obj_t,
	pub(crate) _phantom: PhantomData<&'a Object>
}

#[derive(Copy, Clone)]
#[repr(transparent)]
#[allow(non_camel_case_types)]
pub struct obj_mut<'a> {
	pub raw: *mut lv_obj_t,
	pub(crate) _phantom: PhantomData<&'a mut Object>
}

impl Object {
	pub fn new(parent: Option<obj_mut>) -> Self {
		let parent = parent.map_or(ptr::null_mut(), |p| p.raw);
		Self {
			raw: unsafe { lv_obj_create(parent) },
		}
	}

	pub fn downcast<T: Widget>(self) -> Option<T> {
		let class = unsafe { (*self.raw).class_p };
		if class != T::class() { None }
		else {
			unsafe { Some(mem::transmute_copy(&self)) }
		}
	}

	pub fn as_ref(&self) -> obj<'_> {
		unsafe { mem::transmute_copy(self) }
	}

	pub fn as_mut(&mut self) -> obj_mut<'_> {
		unsafe { mem::transmute_copy(self) }
	}
}

auto trait NotBaseObject {}
impl !NotBaseObject for Object {}

impl<T: Widget + NotBaseObject> From<T> for Object {
	fn from(value: T) -> Self {
		value.upcast()
	}
}

impl<'a> Deref for obj<'a> {
	type Target = obj_mut<'a>;

	fn deref(&self) -> &Self::Target {
		unsafe { mem::transmute(self) }
	}
}

impl<'a> Deref for obj_mut<'a> {
	type Target = Object;

	fn deref(&self) -> &Self::Target {
		unsafe { mem::transmute(self) }
	}
}

impl DerefMut for obj_mut<'_> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { mem::transmute(self) }
	}
}

impl Drop for Object {
	fn drop(&mut self) {
		trace!("generic object dropped");
		unsafe { lv_obj_del(self.raw) }
	}
}

/// # Safety
/// The type this is implemented on must be `repr(transparent)` around a `*mut lv_obj_t`
pub unsafe trait Widget {
	fn class() -> *const lv_obj_class_t;

	fn upcast(self) -> Object where Self: Sized {
		unsafe { mem::transmute_copy(&self) }
	}

	fn upcast_ref(&mut self) -> obj<'_> {
		unsafe {
			obj {
				raw: *(self as *mut Self as *mut *const lv_obj_t),
				_phantom: PhantomData
			}
		}
	}

	fn upcast_mut(&mut self) -> obj_mut<'_> {
		unsafe {
			obj_mut {
				raw: *(self as *mut Self as *mut *mut lv_obj_t),
				_phantom: PhantomData
			}
		}
	}

	fn inline_style(&mut self, part: Part, state: State) -> InlineStyle<'_> {
		InlineStyle {
			widget: self.upcast_mut(),
			selector: lv_part_t::from(part) | (lv_state_t::from(state) as u32),
		}
	}

	fn add_style(&mut self, part: Part, state: State, style: &mut ExternalStyle) {
		let raw = self.upcast_mut().raw;
		let selector = lv_part_t::from(part) | (lv_state_t::from(state) as u32);
		unsafe { lv_obj_add_style(raw, &mut style.raw, selector); }
	}
}

unsafe impl Widget for Object {
	fn class() -> *const lv_obj_class_t {
		unsafe { &lvgl_sys::lv_obj_class }
	}
}

impl<'a> obj<'a> {
	pub fn downcast_ref<T: Widget>(self) -> Option<&'a T> {
		let class = unsafe { (*self.raw).class_p };
		if class != T::class() { None }
		else {
			unsafe { Some(mem::transmute(&self)) }
		}
	}
}

impl<'a> obj_mut<'a> {
	pub fn downcast_mut<T: Widget>(mut self) -> Option<&'a mut T> {
		let class = unsafe { (*self.raw).class_p };
		if class != T::class() { None }
		else {
			unsafe { Some(mem::transmute(&mut self)) }
		}
	}
}
