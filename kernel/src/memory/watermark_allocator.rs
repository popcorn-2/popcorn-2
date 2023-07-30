use core::alloc::AllocError;
use core::num::NonZeroUsize;
use kernel_exports::sync::Lock;
use super::{Frame};
use utils::handoff::{MemoryMapEntry, MemoryType};
use crate::{into};
use negative_slice::NegativeSlice;

mod negative_slice {
	use core::ops::{Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};

	#[derive(Debug)]
	#[repr(transparent)]
	pub struct NegativeSlice<T>(pub(in super) [T]);

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
		use macros::test_should_panic;
		use super::NegativeSlice;

		const TEST_SLICE: &NegativeSlice<u8> = NegativeSlice::new(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
		const TEST_SLICE_SINGLE: &NegativeSlice<u8> = NegativeSlice::new(&[0]);

		#[test_case]
		fn normal_single_index() {
			assert_eq!(TEST_SLICE[0], 0);
			assert_eq!(TEST_SLICE[3], 3);
		}

		#[test_should_panic]
		fn out_of_bounds_index() {
			TEST_SLICE[76];
		}

		#[test_case]
		fn reverse_single_index() {
			assert_eq!(TEST_SLICE[-1], 10);
			assert_eq!(TEST_SLICE[-5], 6);
		}

		#[test_should_panic]
		fn reverse_single_index_out_of_bounds() {
			TEST_SLICE[-23];
		}

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

		#[test_should_panic]
		fn backwards_range() {
			let _ = &TEST_SLICE[9..-9];
		}

		#[test_case]
		fn zero_length_struct_from_up_to_minus_one() {
			assert_eq!(&TEST_SLICE_SINGLE[..-1].0, []);
		}
	}
}

#[derive(Debug)]
pub struct WatermarkAllocator<'mem_map>(Lock<WatermarkAllocatorInner<'mem_map>>);

impl<'mem_map> WatermarkAllocator<'mem_map> {
	pub fn new<E: AsRef<[MemoryMapEntry]> + ?Sized>(mem_map: &'mem_map E) -> Self {
		Self(Lock::new(WatermarkAllocatorInner::new(mem_map)))
	}

	pub fn allocate_contiguous(&self, page_count: NonZeroUsize, alignment_log2: usize) -> Result<Frame, AllocError> {
		self.0.lock().allocate_contiguous(page_count, alignment_log2)
	}
}

#[derive(Debug)]
pub struct WatermarkAllocatorInner<'mem_map> {
	mem_map: &'mem_map NegativeSlice<MemoryMapEntry>,
	prev_frame: Frame
}

impl<'mem_map> WatermarkAllocatorInner<'mem_map> {
	fn current_area(&self) -> &'mem_map MemoryMapEntry {
		&self.mem_map[-1]
	}

	pub fn new<Entry: AsRef<[MemoryMapEntry]> + ?Sized>(mem_map: &'mem_map Entry) -> Self {
		let mem_map = mem_map.as_ref();
		let (last_free_section, &last_free_address) = mem_map.iter().enumerate().rev()
				.find(|(_, entry)| entry.ty == MemoryType::Free)
				.expect("Unable to find any free memory");
		Self {
			mem_map: &NegativeSlice::new(mem_map)[..into!(last_free_section)],
			prev_frame: Frame::align_down(last_free_address.coverage.end())
		}
	}

	pub fn allocate_contiguous(&mut self, page_count: NonZeroUsize, alignment_log2: usize) -> Result<Frame, AllocError> {
		if alignment_log2 != 0 { todo!("Higher than 4K alignment") }

		let mut test_frame = self.prev_frame - page_count.get();

		loop {
			if test_frame.start() >= self.current_area().coverage.start() { break; }
			else {
				while self.current_area().ty != MemoryType::Free {
					if self.mem_map.0.len() == 0 { return Err(AllocError); }
					self.mem_map = &self.mem_map[..-1];
				}
				let end_frame = Frame::align_down(self.current_area().end());
				test_frame = end_frame - page_count.get();
			}
		}

		self.prev_frame = test_frame;
		Ok(test_frame)
	}
}
