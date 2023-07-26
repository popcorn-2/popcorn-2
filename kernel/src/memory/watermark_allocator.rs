use alloc::vec::Vec;
use core::cell::RefCell;
use core::ops::Range;
use super::{Frame, PhysicalAddress, PhysicalMemoryAllocator};
use utils::handoff::{MemoryMapEntry, MemoryType};

#[derive(Debug)]
pub struct WatermarkAllocator<'mem_map>(RefCell<WatermarkAllocatorInner<'mem_map>>);

impl<'mem_map> WatermarkAllocator<'mem_map> {
	pub fn new(mem_map: &'mem_map mut Vec<MemoryMapEntry>) -> Self {
		Self(RefCell::new(WatermarkAllocatorInner::new(mem_map)))
	}
}

#[derive(Debug)]
pub struct WatermarkAllocatorInner<'mem_map> {
	mem_map: &'mem_map mut Vec<MemoryMapEntry>,
	current_entry_idx: usize,
	prev_frame: Frame
}

impl<'mem_map> WatermarkAllocatorInner<'mem_map> {
	pub fn new(mem_map: &'mem_map mut Vec<MemoryMapEntry>) -> Self {
		let (last_free_section, &last_free_address) = mem_map.iter().enumerate().rev()
				.find(|(_, entry)| entry.ty == MemoryType::Free)
				.expect("Unable to find any free memory");
		Self {
			mem_map,
			current_entry_idx: last_free_section,
			prev_frame: Frame::align_down(PhysicalAddress(last_free_address.coverage.1.try_into().unwrap()))
		}
	}

	fn allocate_frame(&mut self) -> Result<AllocationWithMetadata, ()> {
		let mut jumped = false;
		let mut test_frame = self.prev_frame - 1;
		if u64::try_from(test_frame.start()).unwrap() < self.mem_map[self.current_entry_idx].coverage.0 {
			jumped = true;
			loop {
				if self.current_entry_idx == 0 { return Err(()); }
				self.current_entry_idx -= 1;
				if self.mem_map[self.current_entry_idx].ty == MemoryType::Free { break; }
			}
			test_frame = Frame::align_down(PhysicalAddress(self.mem_map[self.current_entry_idx].coverage.1.try_into().unwrap()));
		}
		self.prev_frame = test_frame;
		Ok(AllocationWithMetadata {
			frame: test_frame,
			contiguous: !jumped
		})
	}
}

impl<'mem_map> PhysicalMemoryAllocator for WatermarkAllocator<'mem_map> {
	fn try_new(_: Range<Frame>) -> Result<Self, ()> { unimplemented!() }

	fn allocate_contiguous(&self, page_count: usize) -> Result<Frame, ()> {
		let mut i = page_count;

		if i == 0 { return Err(()); }

		let mut inner = self.0.try_borrow_mut().map_err(|_| ())?;
		let mut frame = Frame::new(PhysicalAddress(0));

		while i > 0 {
			let alloc = inner.allocate_frame()?;
			frame = alloc.frame;
			if alloc.contiguous { i -= 1; }
			else { i = page_count; }
		}

		Ok(frame)
	}
}

struct AllocationWithMetadata {
	frame: Frame,
	contiguous: bool
}
