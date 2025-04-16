use core::cell::OnceCell;
#[allow(unused_imports)] use crate::prelude::*;
use kernel_api::time::Instant;
use crate::hal::interrupts_v2::Vector;

pub(super) fn init_local_timer(timer: TimerMeta) {
	match percpu_v2!(local_timer).try_insert(timer) {
		Ok(_) => {},
		Err(_) => panic!("`LOCAL_TIMER` already initialised"),
	}
}

pub fn local_timer() -> &'static TimerMeta {
	percpu_v2!(local_timer).get().expect("`local_timer` not yet initialised")
}

pub trait Timer {
	fn mask(&self, masked: bool);
	fn set_deadline(&self, time: Instant) -> Result<(), ()>;
}

pub struct TimerMeta {
	vector: Vector,
	mask: fn(*const (), bool),
	set_deadline: fn(*const (), Instant) -> Result<(), ()>,
	data: *const (),
}

impl TimerMeta {
	pub const fn new<T: Timer>(vector: Vector, interface: &'static T) -> Self {
		Self {
			vector,
			mask: unsafe { core::mem::transmute(T::mask as fn(&T, bool)) },
			set_deadline: unsafe { core::mem::transmute(T::set_deadline as fn(&T, Instant) -> Result<(), ()>) },
			data: interface as *const T as _
		}
	}
	
	pub const fn vector(&self) -> Vector { self.vector }
	
	pub fn mask(&self, masked: bool) {
		(self.mask)(self.data, masked)
	}

	pub fn set_deadline(&self, time: Instant) -> Result<(), ()> {
		(self.set_deadline)(self.data, time)
	}

	pub fn data(&self) -> *const () {
		self.data
	}
}
