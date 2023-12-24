use alloc::sync::Arc;
use core::num::NonZeroUsize;
use core::ops::Range;
use log::trace;
use kernel_api::memory::allocator::{AllocationMeta, BackingAllocator, Config};
use kernel_api::memory::{Frame, PhysicalAddress, AllocError};
use kernel_api::sync::Mutex;

/*mod negative_slice {
	use core::ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};

	#[derive(Debug)]
	#[repr(transparent)]
	pub struct NegativeSlice<T>(pub(crate) [T]);

	impl<T> NegativeSlice<T> {
		pub const fn new(a: &[T]) -> &Self { unsafe { core::mem::transmute(a) } }
		pub fn new_mut(a: &mut [T]) -> &mut Self { unsafe { core::mem::transmute(a) } }
	}

	impl<T> core::ops::Index<Range<isize>> for NegativeSlice<T> {
		type Output = Self;

		fn index(&self, index: Range<isize>) -> &Self {
			let start = if index.start >= 0 { index.start as usize } else { self.0.len().checked_add_signed(index.start).unwrap() };
			let end = if index.end >= 0 { index.end as usize } else { self.0.len().checked_add_signed(index.end).unwrap() };

			unsafe { core::mem::transmute(&self.0[start..end]) }
		}
	}

	impl<T> core::ops::IndexMut<Range<isize>> for NegativeSlice<T> {
		fn index_mut(&mut self, index: Range<isize>) -> &mut Self {
			let start = if index.start >= 0 { index.start as usize } else { self.0.len().checked_add_signed(index.start).unwrap() };
			let end = if index.end >= 0 { index.end as usize } else { self.0.len().checked_add_signed(index.end).unwrap() };

			unsafe { core::mem::transmute(&mut self.0[start..end]) }
		}
	}

	impl<T> core::ops::Index<RangeFrom<isize>> for NegativeSlice<T> {
		type Output = Self;

		fn index(&self, index: RangeFrom<isize>) -> &Self {
			let start = if index.start >= 0 { index.start as usize } else { self.0.len().checked_add_signed(index.start).unwrap() };

			unsafe { core::mem::transmute(&self.0[start..]) }
		}
	}

	impl<T> core::ops::IndexMut<RangeFrom<isize>> for NegativeSlice<T> {
		fn index_mut(&mut self, index: RangeFrom<isize>) -> &mut Self {
			let start = if index.start >= 0 { index.start as usize } else { self.0.len().checked_add_signed(index.start).unwrap() };

			unsafe { core::mem::transmute(&mut self.0[start..]) }
		}
	}

	impl<T> core::ops::Index<RangeTo<isize>> for NegativeSlice<T> {
		type Output = Self;

		fn index(&self, index: RangeTo<isize>) -> &Self {
			let end = if index.end >= 0 { index.end as usize } else { self.0.len().checked_add_signed(index.end).unwrap() };

			unsafe { core::mem::transmute(&self.0[..end]) }
		}
	}

	impl<T> core::ops::IndexMut<RangeTo<isize>> for NegativeSlice<T> {
		fn index_mut(&mut self, index: RangeTo<isize>) -> &mut Self {
			let end = if index.end >= 0 { index.end as usize } else { self.0.len().checked_add_signed(index.end).unwrap() };

			unsafe { core::mem::transmute(&mut self.0[..end]) }
		}
	}

	impl<T> core::ops::Index<RangeToInclusive<isize>> for NegativeSlice<T> {
		type Output = Self;

		fn index(&self, index: RangeToInclusive<isize>) -> &Self {
			let end = if index.end >= 0 { index.end as usize } else { self.0.len().checked_add_signed(index.end).unwrap() };

			unsafe { core::mem::transmute(&self.0[..=end]) }
		}
	}

	impl<T> core::ops::IndexMut<RangeToInclusive<isize>> for NegativeSlice<T> {
		fn index_mut(&mut self, index: RangeToInclusive<isize>) -> &mut Self {
			let end = if index.end >= 0 { index.end as usize } else { self.0.len().checked_add_signed(index.end).unwrap() };

			unsafe { core::mem::transmute(&mut self.0[..=end]) }
		}
	}

	impl<T> core::ops::Index<RangeInclusive<isize>> for NegativeSlice<T> {
		type Output = Self;

		fn index(&self, index: RangeInclusive<isize>) -> &Self {
			let start = if *index.start() >= 0 { *index.start() as usize } else { self.0.len().checked_add_signed(*index.start()).unwrap() };
			let end = if *index.end() >= 0 { *index.end() as usize } else { self.0.len().checked_add_signed(*index.end()).unwrap() };

			unsafe { core::mem::transmute(&self.0[start..=end]) }
		}
	}

	impl<T> core::ops::IndexMut<RangeInclusive<isize>> for NegativeSlice<T> {
		fn index_mut(&mut self, index: RangeInclusive<isize>) -> &mut Self {
			let start = if *index.start() >= 0 { *index.start() as usize } else { self.0.len().checked_add_signed(*index.start()).unwrap() };
			let end = if *index.end() >= 0 { *index.end() as usize } else { self.0.len().checked_add_signed(*index.end()).unwrap() };

			unsafe { core::mem::transmute(&mut self.0[start..=end]) }
		}
	}

	impl<T> core::ops::Index<RangeFull> for NegativeSlice<T> {
		type Output = Self;

		fn index(&self, _: RangeFull) -> &Self {
			unsafe { core::mem::transmute(&self.0[..]) }
		}
	}

	impl<T> core::ops::IndexMut<RangeFull> for NegativeSlice<T> {
		fn index_mut(&mut self, _: RangeFull) -> &mut Self {
			unsafe { core::mem::transmute(&mut self.0[..]) }
		}
	}

	impl<T> core::ops::Index<isize> for NegativeSlice<T> {
		type Output = T;

		fn index(&self, index: isize) -> &T {
			let index = if index >= 0 { index as usize } else { self.0.len().checked_add_signed(index).unwrap() };
			unsafe { core::mem::transmute(&self.0[index]) }
		}
	}

	impl<T> core::ops::IndexMut<isize> for NegativeSlice<T> {
		fn index_mut(&mut self, index: isize) -> &mut T {
			let index = if index >= 0 { index as usize } else { self.0.len().checked_add_signed(index).unwrap() };
			unsafe { core::mem::transmute(&mut self.0[index]) }
		}
	}

	#[cfg(test)]
	mod tests {
		//use macros::test_should_panic;
		use super::NegativeSlice;

		const TEST_SLICE: &NegativeSlice<u8> = NegativeSlice::new(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
		const TEST_SLICE_SINGLE: &NegativeSlice<u8> = NegativeSlice::new(&[0]);

		#[test_case]
		fn normal_single_index() {
			assert_eq!(TEST_SLICE[0], 0);
			assert_eq!(TEST_SLICE[3], 3);
		}

		/*#[test_should_panic]
		fn out_of_bounds_index() {
			TEST_SLICE[76];
		}*/

		#[test_case]
		fn reverse_single_index() {
			assert_eq!(TEST_SLICE[-1], 10);
			assert_eq!(TEST_SLICE[-5], 6);
		}

		/*#[test_should_panic]
		fn reverse_single_index_out_of_bounds() {
			TEST_SLICE[-23];
		}*/

		#[test_case]
		fn forward_ranges() {
			assert_eq!(&TEST_SLICE[0..3].0, [0, 1, 2]);
			assert_eq!(&TEST_SLICE[0..=3].0, [0, 1, 2, 3]);
			assert_eq!(&TEST_SLICE[7..].0, [7, 8, 9, 10]);
			assert_eq!(&TEST_SLICE[..5].0, [0, 1, 2, 3, 4]);
			assert_eq!(&TEST_SLICE[..=5].0, [0, 1, 2, 3, 4, 5]);
			assert_eq!(&TEST_SLICE[..].0, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
		}

		#[test_case]
		fn reverse_ranges() {
			assert_eq!(&TEST_SLICE[-5..-1].0, [6, 7, 8, 9]);
			assert_eq!(&TEST_SLICE[-5..=-1].0, [6, 7, 8, 9, 10]);
			assert_eq!(&TEST_SLICE[-4..].0, [7, 8, 9, 10]);
			assert_eq!(&TEST_SLICE[..-7].0, [0, 1, 2, 3]);
			assert_eq!(&TEST_SLICE[..=-7].0, [0, 1, 2, 3, 4]);
		}

		#[test_case]
		fn mixed_ranges() {
			assert_eq!(&TEST_SLICE[3..-5].0, [3, 4, 5]);
			assert_eq!(&TEST_SLICE[-5..9].0, [6, 7, 8]);
		}

		/*#[test_should_panic]
		fn backwards_range() {
			let _ = &TEST_SLICE[9..-9];
		}*/

		#[test_case]
		fn zero_length_struct_from_up_to_minus_one() {
			assert_eq!(&TEST_SLICE_SINGLE[..-1].0, []);
		}
	}
}*/

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
