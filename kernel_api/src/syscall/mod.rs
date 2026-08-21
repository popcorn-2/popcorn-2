pub mod handle;
mod result;
pub mod server;

pub use result::*;
use crate::channel;
use crate::channel::Receiver;

#[derive(Debug)]
pub struct AsyncMap {
	pub queue: Receiver<(usize, core::result::Result<u128, Error>)>,
}

impl AsyncMap {
	pub fn new() -> Self {
		Self {
			queue: channel::unbounded().1,
		}
	}

	pub fn push_result(&self, key: usize, result: core::result::Result<u128, Error>) {
		self.queue.push((key, result));
	}
}

impl Default for AsyncMap {
	fn default() -> Self {
		Self::new()
	}
}
