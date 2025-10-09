use kernel_api::allocator::AllocError;
use kernel_api::mapping::Ty;
use kernel_api::memory::RawFrame;
use crate::hal::paging2::Flags;
//pub mod levels;

pub trait Entry: Copy + Default {
	fn is_present(self) -> bool;
	fn pointed_frame(self, debug: bool) -> Option<RawFrame>;
	fn point_to_frame(&mut self, frame: RawFrame, ty: Ty, flags: Flags) -> Result<(), Ty>;
}

#[derive(Debug)]
pub enum MapPageError {
	AlreadyMapped(#[expect(unused)] Ty),
	AllocError,
}

impl From<AllocError> for MapPageError {
	fn from(_value: AllocError) -> Self {
		Self::AllocError
	}
}
