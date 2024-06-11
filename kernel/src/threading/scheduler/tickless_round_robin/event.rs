#[allow(unused_imports)] use crate::prelude::*;
use core::cmp::Ordering;
use kernel_api::time::Instant;
use crate::threading::ThreadId;

#[derive(Debug, Copy, Clone)]
pub struct Event {
	time: Instant,
	tid: ThreadId,
	action: Ty,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Ty {
	Unblock,
}

impl PartialEq for Event {
	fn eq(&self, other: &Self) -> bool {
		self.time == other.time
	}
}

impl PartialOrd for Event {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		self.time.partial_cmp(&other.time)
	}
}

#[derive(Debug)]
pub struct Queue {
	#[cfg(feature = "preemptive")] yield_event: Option<Yield>,
}

#[derive(Debug)]
pub struct Yield {
	time: Instant,
	thread: ThreadId,
}

impl Queue {
	pub fn new() -> Queue {
		Queue {
			#[cfg(feature = "preemptive")] yield_event: None
		}
	}
	
	pub fn peek_next(&self) -> Event {
		todo!()
	}

	pub fn take_next(&self) -> Event {
		todo!()
	}
}
