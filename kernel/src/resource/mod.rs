use crate::resource::arc_frames::OwnedFrames;

pub mod watermark_allocator;
//pub mod background_zeroer;

pub trait FrameAllocator {
	fn allocate(&self, count: usize) -> Result<OwnedFrames<'_>, AllocError>;
	fn deallocate(&self, frames: &OwnedFrames);
}

pub use kernel_api::memory::allocator::AllocError;

mod arc_frames {
	use core::fmt;
	use log::trace;
	use super::FrameAllocator;
	use kernel_api::memory::Frame;

	// Conceptually the same as Arc<[Frame]>
	pub struct OwnedFrames<'allocator> {
		start: Frame,
		count: usize,
		allocator: &'allocator dyn FrameAllocator
	}

	impl<'allocator> fmt::Debug for OwnedFrames<'allocator> {
		fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
			f.debug_struct("OwnedFrames")
					.field("start", &self.start)
					.field("count", &self.count)
					.finish_non_exhaustive()
		}
	}

	impl<'allocator> OwnedFrames<'allocator> {
		pub fn new(start: Frame, count: usize, allocator: &'allocator dyn FrameAllocator) -> Self {
			trace!("Ownership of frames {:?}..{:?} started", start, start + count);
			Self {
				start,
				count,
				allocator
			}
		}
	}

	impl<'allocator> Drop for OwnedFrames<'allocator> {
		fn drop(&mut self) {
			trace!("Ownership of frames {:?}..{:?} dropped", self.start, self.start + self.count);
			self.allocator.deallocate(self)
		}
	}

	impl<'allocator> Clone for OwnedFrames<'allocator> {
		fn clone(&self) -> Self {
			todo!("Owned frames not yet refcounted")
		}
	}
}
