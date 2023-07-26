use core::alloc::{Allocator, AllocError};
use core::fmt::Debug;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::ops::Range;
use derive_more::Display;

#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Debug)]
pub struct Page {
	pub number: usize
}
#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Debug)]
pub struct Frame {
	pub number: usize
}
#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Debug)]
pub struct PhysicalAddress(pub usize);
#[derive(Ord, PartialOrd, Eq, PartialEq, Copy, Clone, Debug)]
pub struct VirtualAddress(pub usize);

impl Frame {
	pub fn try_new(PhysicalAddress(addr): PhysicalAddress) -> Result<Frame, AlignError> {
		if addr == (addr & !0xfff) { Ok(Frame{ number: addr / 4096 }) }
		else { Err(AlignError) }
	}

	#[track_caller]
	#[inline]
	pub fn new(addr: PhysicalAddress) -> Frame {
		match Frame::try_new(addr) {
			Ok(f) => f,
			Err(e) => panic!("{e}")
		}
	}

	#[inline]
	pub unsafe fn new_unchecked(PhysicalAddress(addr): PhysicalAddress) -> Frame {
		Frame{ number: addr / 4096 }
	}

	#[inline]
	pub fn align_down(PhysicalAddress(addr): PhysicalAddress) -> Frame {
		unsafe { Frame::new_unchecked(PhysicalAddress(addr & !0xfff)) }
	}

	#[inline]
	pub fn start(self) -> usize {
		self.number * 4096
	}
}

/*impl Into<u64> for Frame {
	#[inline]
	fn into(self) -> u64 {
		self.number.try_into().unwrap()
	}
}

impl Into<usize> for Frame {
	#[inline]
	fn into(self) -> usize {
		self.number
	}
}*/

#[derive(Debug, Display)]
#[display(fmt = "Address was not aligned to a 4K boundary")]
pub struct AlignError;

impl core::error::Error for AlignError {}

pub unsafe trait PhysicalMemoryAllocator: Debug {
	//fn new_from(allocator: &dyn PhysicalMemoryAllocator, coverage: Range<Frame>) -> Result<Self, ()> where Self: Sized;
	fn allocate_contiguous(&self, page_count: usize) -> Result<Frame, AllocError>;
	fn deallocate(&self, start: Frame, page_count: usize);
	fn get_allocated_regions(&self) -> AllocatedRegionIter;
	fn get_free_regions(&self) -> FreeRegionIter;
}

pub struct AllocatedRegionIter;

impl Iterator for AllocatedRegionIter {
	type Item = Range<Frame>;

	fn next(&mut self) -> Option<Self::Item> {
		todo!()
	}
}

pub struct FreeRegionIter;

impl Iterator for FreeRegionIter {
	type Item = Range<Frame>;

	fn next(&mut self) -> Option<Self::Item> {
		todo!()
	}
}

impl core::ops::Sub<Frame> for Frame {
	type Output = usize;

	#[track_caller]
	#[inline]
	fn sub(self, rhs: Frame) -> Self::Output {
		self.number - rhs.number
	}
}

impl core::ops::Add<usize> for Frame {
	type Output = Frame;

	#[track_caller]
	#[inline]
	fn add(self, rhs: usize) -> Self::Output {
		Frame { number: self.number + rhs }
	}
}

impl core::ops::Sub<usize> for Frame {
	type Output = Frame;

	#[track_caller]
	#[inline]
	fn sub(self, rhs: usize) -> Self::Output {
		Frame { number: self.number - rhs }
	}
}
