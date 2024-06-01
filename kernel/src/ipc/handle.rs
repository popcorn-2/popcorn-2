#[allow(unused_imports)] use crate::prelude::*;
use crate::ipc::{Error, NonNegativeIsize};
use crate::ipc::server::ServerId;

pub struct Handle {
	server_id: ServerId,
	internal_id: u16,
}

impl Handle {
	pub fn new(server_id: ServerId, internal_id: u16) -> Handle {
		Handle { server_id, internal_id }
	}
	
	pub fn from_arg(arg: usize) -> Result<Self, Error> {
		const HALF_BITS: usize = core::mem::size_of::<usize>() * 8 / 2;
		let lower = arg & !(1 << HALF_BITS);
		let upper = arg >> HALF_BITS;
		
		let server_id = ServerId::new(
			lower.try_into().map_err(|_| Error::InvalidArg)?
		);
		let internal_id = upper.try_into().map_err(|_| Error::InvalidArg)?;
		Ok(Self {
			server_id,
			internal_id,
		})
	}

	pub fn to_arg(self) -> NonNegativeIsize {
		const HALF_BITS: usize = core::mem::size_of::<usize>() * 8 / 2;
		let lower = usize::from(self.server_id.get());
		let upper = usize::from(self.internal_id);
		
		NonNegativeIsize::new((upper << HALF_BITS | lower) as isize)
				.expect("Handle should have ")
	}
}
