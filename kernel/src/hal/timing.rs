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
	fn mask(&mut self, masked: bool);
	fn set_deadline(&mut self, time: Instant) -> Result<(), ()>;
}

pub struct TimerMeta {
	vector: Vector,
	mask: fn(*mut (), bool),
	set_deadline: fn(*mut (), Instant) -> Result<(), ()>,
	data: *mut (),
}

impl TimerMeta {
	pub const fn new<T: Timer>(vector: Vector, interface: &'static mut T) -> Self {
		Self {
			vector,
			mask: unsafe { core::mem::transmute(T::mask as fn(&mut T, bool)) },
			set_deadline: unsafe { core::mem::transmute(T::set_deadline as fn(&mut T, Instant) -> Result<(), ()>) },
			data: interface as *mut T as _
		}
	}
	
	pub const fn vector(&self) -> Vector { self.vector }

	#[expect(dead_code)]
	pub fn mask(&mut self, masked: bool) {
		(self.mask)(self.data, masked)
	}

	#[expect(dead_code)]
	pub fn set_deadline(&mut self, time: Instant) -> Result<(), ()> {
		(self.set_deadline)(self.data, time)
	}
}
