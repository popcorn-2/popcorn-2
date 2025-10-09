use core::marker::PhantomData;
use core::ops::{Index, IndexMut};
use kernel_api::allocator::{AllocError, Pmm};
use kernel_api::mapping::Ty;
use kernel_api::memory::{RawFrame, RawPage};
use crate::hal::arch::amd64::paging::entry::Amd64Entry;
use crate::hal::paging2::Flags;
use crate::hal::paging::Entry;

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
	#[allow(clippy::unusual_byte_groupings)] const MASK: usize = 0o777_000_000_000_0000;
	const SHIFT: usize = 12 + 9*3;
}

impl Level for PDPT {
	#[allow(clippy::unusual_byte_groupings)] const MASK: usize = 0o777_000_000_0000;
	const SHIFT: usize = 12 + 9*2;
}

impl Level for PD {
	#[allow(clippy::unusual_byte_groupings)] const MASK: usize = 0o777_000_0000;
	const SHIFT: usize = 12 + 9;
}

impl Level for PT {
	#[allow(clippy::unusual_byte_groupings)] const MASK: usize = 0o777_0000;
	const SHIFT: usize = 12;
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

impl<L: Level> Index<RawPage> for Table<L> {
	type Output = Amd64Entry;

	fn index(&self, page: RawPage) -> &Amd64Entry {
		&self.entries[page.index::<L>()]
	}
}

impl<L: Level> IndexMut<RawPage> for Table<L> {
	fn index_mut(&mut self, page: RawPage) -> &mut Amd64Entry {
		&mut self.entries[page.index::<L>()]
	}
}

impl<L: Level> Table<L> {
	fn empty() -> Self {
		Self {
			entries: [Amd64Entry::empty(); 512],
			_phantom: PhantomData,
		}
	}
	
	pub(super) fn empty_with(allocator: impl Pmm<true>) -> Result<RawFrame, AllocError> {
		let table_frame = allocator.allocate_one_raw()?;
		let ptr = table_frame.to_virtual().as_ptr();
		unsafe { ptr.cast::<Self>().write(Self::empty()) };

		Ok(table_frame)
	}

	pub(super) fn is_empty(&self) -> bool {
		self.entries.iter().all(|entry| !entry.is_used())
	}
}

impl<L: ParentLevel> Table<L> {
	pub(super) fn child_table(&self, page: RawPage) -> Option<&Table<L::Child>> {
		let entry = self[page];
		let table_frame = entry.pointed_frame(true)?;
		let table_page = table_frame.to_virtual().as_ptr();
		Some(unsafe { &*table_page.cast() })
	}

	pub(super) fn child_table_mut(&mut self, page: RawPage) -> Option<&mut Table<L::Child>> {
		let entry = self[page];
		let table_frame = entry.pointed_frame(false)?;
		let table_page = table_frame.to_virtual().as_ptr();
		Some(unsafe { &mut *table_page.cast() })
	}

	pub(super) fn child_table_or_new(&mut self, page: RawPage, allocator: impl Pmm<true>) -> Result<&mut Table<L::Child>, AllocError> {
		if self.child_table_mut(page).is_none() {
			let table_frame = Table::<L::Child>::empty_with(allocator)?;
			self[page].point_to_frame(table_frame, Ty::PAGE_TABLE, Flags::WRITE | Flags::USER | Flags::EXEC).expect("Entry was not present");
		}
		
		Ok(self.child_table_mut(page).expect("Just mapped this entry"))
	}
}

pub trait PageIndices {
	fn index<L: Level>(self) -> usize;
}

impl PageIndices for RawPage {
	fn index<L: Level>(self) -> usize {
		(self.addr & L::MASK) >> L::SHIFT
	}
}
