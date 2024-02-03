use core::marker::PhantomData;
use kernel_api::memory::allocator::BackingAllocator;
use kernel_api::memory::{AllocError, Frame, Page};
use crate::arch::amd64::paging::Amd64Entry;
use crate::paging::Entry;

pub(super) trait Level {
	const MASK: usize;
	const SHIFT: usize;
}

pub(super) trait ParentLevel: Level {
	type Child: Level;
}

#[derive(Debug)]
pub(super) enum PML4 {}

#[derive(Debug)]
pub(super) enum PDPT {}

#[derive(Debug)]
pub(super) enum PD {}

#[derive(Debug)]
pub(super) enum PT {}

impl Level for PML4 {
	const MASK: usize = 0o777_000_000_000_0000;
	const SHIFT: usize = 12 + 9*3;
}


impl Level for PDPT {
	const MASK: usize = 0o777_000_000_0000;
	const SHIFT: usize = 12 + 9*2;
}


impl Level for PD {
	const MASK: usize = 0o777_000_0000;
	const SHIFT: usize = 12 + 9*1;
}


impl Level for PT {
	const MASK: usize = 0o777_0000;
	const SHIFT: usize = 12 + 9*0;
}

impl ParentLevel for PML4 {
	type Child = PDPT;
}

impl ParentLevel for PDPT {
	type Child = PD;
}

impl ParentLevel for PD {
	type Child = PT;
}

#[derive(Debug)]
#[repr(C, align(4096))]
pub(super) struct Table<L> {
	pub(super) entries: [Amd64Entry; 512],
	_phantom: PhantomData<L>,
}

impl<L: Level> Table<L> {
	pub(super) fn empty() -> Self {
		Self {
			entries: [Amd64Entry(0); 512],
			_phantom: PhantomData
		}
	}

	pub(super) fn empty_with(allocator: impl BackingAllocator) -> Result<Frame, AllocError> {
		let table_frame = allocator.allocate_one()?;
		let table_ptr: *mut Self = table_frame.to_page().as_ptr().cast();
		assert!(!table_ptr.is_null() && table_ptr.is_aligned());

		unsafe { table_ptr.write(Table::empty()); }

		Ok(table_frame)
	}

	pub(super) fn is_empty(&self) -> bool {
		self.entries.iter().all(|entry| !entry.is_used())
	}
}

impl<L: ParentLevel> Table<L> {
	pub(super) fn child_table(&self, idx: usize) -> Option<&Table<L::Child>> {
		let entry = self.entries[idx];
		let table_frame = entry.pointed_frame()?;
		let table_page = table_frame.to_page();
		Some(unsafe { &*table_page.as_ptr().cast() })
	}

	pub(super) fn child_table_mut(&mut self, idx: usize) -> Option<&mut Table<L::Child>> {
		let entry = self.entries[idx];
		let table_frame = entry.pointed_frame()?;
		let table_page = table_frame.to_page();
		Some(unsafe { &mut *table_page.as_ptr().cast() })
	}

	pub(super) fn child_table_or_new(&mut self, idx: usize, allocator: impl BackingAllocator) -> Result<&mut Table<L::Child>, AllocError> {
		if self.child_table_mut(idx).is_none() {
			let table_frame = Table::<L::Child>::empty_with(allocator)?;
			self.entries[idx].point_to_frame(table_frame).expect("Entry was not present");
		}

		Ok(self.child_table_mut(idx).expect("Just mapped this entry"))
	}
}

pub trait PageIndices {
	fn pml4_index(self) -> usize;
	fn pdpt_index(self) -> usize;
	fn pd_index(self) -> usize;
	fn pt_index(self) -> usize;
}

impl PageIndices for Page {
	fn pml4_index(self) -> usize {
		(self.start().addr & PML4::MASK) >> PML4::SHIFT
	}

	fn pdpt_index(self) -> usize {
		(self.start().addr & PDPT::MASK) >> PDPT::SHIFT
	}

	fn pd_index(self) -> usize {
		(self.start().addr & PD::MASK) >> PD::SHIFT
	}

	fn pt_index(self) -> usize {
		(self.start().addr & PT::MASK) >> PT::SHIFT
	}
}
