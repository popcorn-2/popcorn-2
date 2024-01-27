use core::marker::PhantomData;
use kernel_api::memory::allocator::BackingAllocator;
use kernel_api::memory::{AllocError, Frame};
use crate::arch::amd64::paging::Amd64Entry;
use crate::paging::Entry;

pub(super) trait Level {}

pub(super) trait ParentLevel: Level {
	type Child: Level;
}

pub(super) enum PML4 {}

pub(super) enum PDPT {}

pub(super) enum PD {}

pub(super) enum PT {}

impl Level for PML4 {}

impl Level for PDPT {}

impl Level for PD {}

impl Level for PT {}

impl ParentLevel for PML4 {
	type Child = PDPT;
}

impl ParentLevel for PDPT {
	type Child = PD;
}

impl ParentLevel for PD {
	type Child = PT;
}

#[repr(C, align(4096))]
pub(super) struct Table<L> {
	pub(super) entries: [Amd64Entry; 512],
	_phantom: PhantomData<L>,
}

impl<L: Level> Table<L> {
	pub(super) fn new() -> Self {
		Self {
			entries: [Amd64Entry::empty(); 512],
			_phantom: PhantomData
		}
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
			let table_frame: Frame = todo!();
			self.entries[idx].point_to_frame(table_frame).expect("Entry was not present");
		}

		Ok(self.child_table_mut(idx).expect("Just mapped this entry"))
	}
}
