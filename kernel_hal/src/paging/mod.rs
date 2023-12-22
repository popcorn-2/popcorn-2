use core::ptr::NonNull;
use kernel_api::memory::allocator::{AllocError, BackingAllocator};
use kernel_api::memory::{Frame, Page};
use crate::paging::levels::{Lower, Middle, Upper, Global, ParentLevel};

pub mod levels;

pub trait Level: levels::LevelInternal {
	const MASK: usize;
	const SHIFT: usize;

	type Entry: Entry;
	const ENTRY_COUNT: usize;
}

pub trait Entry: Copy {
	fn empty() -> Self;
	fn is_present(self) -> bool;
	fn pointed_frame(self) -> Option<Frame>;
	fn point_to_frame(&mut self, frame: Frame) -> Result<(), ()>;
}

mod sanity {
	use super::Level;

	trait SanityCheck: Level {}
	impl SanityCheck for super::levels::Global {}
	impl SanityCheck for super::levels::Upper {}
	impl SanityCheck for super::levels::Middle {}
	impl SanityCheck for super::levels::Lower {}
}

pub trait PageIndices {
	fn global_index(self) -> usize;
	fn upper_index(self) -> usize;
	fn middle_index(self) -> usize;
	fn lower_index(self) -> usize;
}

impl PageIndices for Page {
	fn global_index(self) -> usize {
		(self.start().addr & Global::MASK) >> Global::SHIFT
	}

	fn upper_index(self) -> usize {
		(self.start().addr & Upper::MASK) >> Upper::SHIFT
	}

	fn middle_index(self) -> usize {
		(self.start().addr & Middle::MASK) >> Middle::SHIFT
	}

	fn lower_index(self) -> usize {
		(self.start().addr & Lower::MASK) >> Lower::SHIFT
	}
}

#[repr(C)]
pub struct Table<L: Level> where [(); L::ENTRY_COUNT]: {
	pub entries: [L::Entry; L::ENTRY_COUNT]
}

impl<L: Level> Table<L> where [(); L::ENTRY_COUNT]: {
	pub fn empty() -> Self {
		Self {
			entries: [L::Entry::empty(); L::ENTRY_COUNT]
		}
	}

	pub fn empty_with(allocator: impl BackingAllocator) -> Result<(Frame, NonNull<Self>), AllocError> {
		let table_frame = allocator.allocate_one()?;
		let table_ptr: *mut Self = table_frame.to_page().as_ptr().cast();

		unsafe { table_ptr.write(Table::empty()); }

		Ok((
			table_frame,
			NonNull::new(table_ptr).expect("Just allocated this pointer")
		))
	}
}

impl<L: Level + ParentLevel> Table<L> where L::Child: Level, [(); L::ENTRY_COUNT]:, [(); <L::Child as Level>::ENTRY_COUNT]: {
	pub fn child_table(&self, idx: usize) -> Option<&Table<L::Child>> {
		let entry = self.entries[idx];
		let table_frame = entry.pointed_frame()?;
		let table_page = table_frame.to_page();
		Some(unsafe { &*table_page.as_ptr().cast() })
	}

	pub fn child_table_mut(&mut self, idx: usize) -> Option<&mut Table<L::Child>> {
		let entry = self.entries[idx];
		let table_frame = entry.pointed_frame()?;
		let table_page = table_frame.to_page();
		Some(unsafe { &mut *table_page.as_ptr().cast() })
	}

	pub fn child_table_or_new(&mut self, idx: usize, allocator: impl BackingAllocator) -> Result<&mut Table<L::Child>, AllocError> {
		if self.child_table_mut(idx).is_none() {
			let (table_frame, _) = Table::<L::Child>::empty_with(allocator)?;
			self.entries[idx].point_to_frame(table_frame).expect("Entry was not present");
		}

		Ok(self.child_table_mut(idx).expect("Just mapped this entry"))
	}
}
