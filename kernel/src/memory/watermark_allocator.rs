use core::num::NonZeroUsize;
use core::ops::Range;
use log::trace;
use kernel_api::memory::allocator::{AllocationMeta, BackingAllocator};
use kernel_api::memory::{Frame, PhysicalAddress, AllocError};
use kernel_api::sync::Mutex;

pub struct WatermarkAllocator<'mem_map>(Mutex<Inner<'mem_map>>);

impl<'mem_map> WatermarkAllocator<'mem_map> {
	pub fn new(free_regions: &'mem_map mut (dyn DoubleEndedIterator<Item = Range<Frame>> + Send)) -> Self {
		Self(Mutex::new(Inner::new(free_regions)))
	}
}

unsafe impl BackingAllocator for WatermarkAllocator<'_> {
	fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, AllocError> {
		match NonZeroUsize::new(frame_count) {
			None => Ok(Frame::new(PhysicalAddress::new(0))),
			Some(count) => {
				self.0.lock()
					.allocate_contiguous(count, 0)
			}
		}
	}

	unsafe fn deallocate_contiguous(&self, _: Frame, _: NonZeroUsize) {
		trace!("WatermarkAllocator ignoring request to deallocate");
	}

	fn drain_into(mut self, into: &mut dyn BackingAllocator) where Self: Sized {
		let inner = self.0.into_inner();
		into.push(AllocationMeta {
			region: inner.prev_frame..inner.top
		});
	}
}

pub struct Inner<'mem_map> {
	free_regions: &'mem_map mut (dyn DoubleEndedIterator<Item = Range<Frame>> + Send),
	last_in_current_region: Frame,
	prev_frame: Frame,
	top: Frame
}

impl<'mem_map> Inner<'mem_map> {
	pub fn new(free_regions: &'mem_map mut (dyn DoubleEndedIterator<Item = Range<Frame>> + Send)) -> Inner<'mem_map> {
		let last_free_section = free_regions.next_back()
			.expect("Unable to find any free memory")
			.clone();

		Self {
			free_regions,
			last_in_current_region: last_free_section.start,
			prev_frame: last_free_section.end,
			top: last_free_section.end
		}
	}

	pub fn allocate_contiguous(&mut self, page_count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AllocError> {
		if alignment_log2 != 0 { todo!("Higher than 4K alignment") }

		let mut test_frame = self.prev_frame.checked_sub(page_count.get())
				.ok_or(AllocError)?;

		loop {
			if test_frame >= self.last_in_current_region { break; }

			let Some(new_region) = self.free_regions.next() else { // Get the next region
				return Err(AllocError); // Out of areas to allocate from
			};
			self.last_in_current_region = new_region.start;

			let end_frame = new_region.end;
			test_frame = end_frame.checked_sub(page_count.get())
			                      .ok_or(AllocError)?;
		}

		self.prev_frame = test_frame;
		Ok(test_frame)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use core::num::NonZeroUsize;
	use kernel_api::memory::{Frame, PhysicalAddress};
	use core::ops::Range;

	const MEMORY_LAYOUT: [Range<Frame>; 4] = [
		Frame::new(PhysicalAddress::new(0))..Frame::new(PhysicalAddress::new(0x2000)),
		Frame::new(PhysicalAddress::new(0x6000))..Frame::new(PhysicalAddress::new(0x7000)),
		Frame::new(PhysicalAddress::new(0x8000))..Frame::new(PhysicalAddress::new(0x9000)),
		Frame::new(PhysicalAddress::new(0xa000))..Frame::new(PhysicalAddress::new(0x10000)),
	];

	#[test]
	#[should_panic = "Unable to find any free memory"]
	fn fail_when_empty_memory() {
		Inner::new(&mut [].iter().cloned());
	}

	#[test]
	fn allocates_available_frames_downwards() {
		let mut iter = MEMORY_LAYOUT[0..1].iter().cloned();
		let mut alloc = Inner::new(&mut iter);
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Ok(Frame::new(PhysicalAddress::new(0x1000))));
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Ok(Frame::new(PhysicalAddress::new(0x0000))));
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Err(AllocError));
	}

	#[test]
	fn jumps_between_areas() {
		let mut iter = MEMORY_LAYOUT[0..2].iter().cloned();
		let mut alloc = Inner::new(&mut iter);
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Ok(Frame::new(PhysicalAddress::new(0x6000))));
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Ok(Frame::new(PhysicalAddress::new(0x1000))));
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Ok(Frame::new(PhysicalAddress::new(0x0000))));
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Err(AllocError));
	}

	#[test]
	fn allocates_multiple_pages() {
		let mut iter = MEMORY_LAYOUT[3..4].iter().cloned();
		let mut alloc = Inner::new(&mut iter);
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(3).unwrap(), 0), Ok(Frame::new(PhysicalAddress::new(0xd000))));
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(2).unwrap(), 0), Ok(Frame::new(PhysicalAddress::new(0xb000))));
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Ok(Frame::new(PhysicalAddress::new(0xa000))));
		assert_eq!(alloc.allocate_contiguous(NonZeroUsize::new(1).unwrap(), 0), Err(AllocError));
	}
}
