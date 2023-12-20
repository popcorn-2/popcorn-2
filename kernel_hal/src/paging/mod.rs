use core::ptr::NonNull;
use kernel_api::memory::allocator::{AllocError, BackingAllocator};
use kernel_api::memory::{Frame, Page};
use crate::paging::levels::{L1, L2, L3, L4, ParentLevel};

pub mod levels;

pub trait ImplementedLevel: levels::Level {
	const MASK: usize;
	const SHIFT: usize;

	type Entry: ImplementedEntry;
	const ENTRY_COUNT: usize;
}

pub trait ImplementedEntry: Copy {
	fn empty() -> Self;
	fn is_present(self) -> bool;
	fn pointed_frame(self) -> Option<Frame>;
	fn point_to_frame(&mut self, frame: Frame) -> Result<(), ()>;
}

mod sanity {
	use super::ImplementedLevel;

	trait SanityCheck: ImplementedLevel {}
	impl SanityCheck for super::levels::L4 {}
	impl SanityCheck for super::levels::L3 {}
	impl SanityCheck for super::levels::L2 {}
	impl SanityCheck for super::levels::L1 {}
}

mod amd64 {
	use bitflags::{bitflags, Flags};
	use kernel_api::memory::{Frame, PhysicalAddress};
	use crate::paging::{ImplementedEntry, ImplementedLevel};
	use crate::paging::levels::{L4, L3, L2, L1};

	impl ImplementedLevel for L4 {
		const MASK: usize = 0o777_000_000_000_0000;
		const SHIFT: usize = 12 + 9*3;
		type Entry = Entry;
		const ENTRY_COUNT: usize = 512;
	}

	impl ImplementedLevel for L3 {
		const MASK: usize = 0o777_000_000__0000;
		const SHIFT: usize = 12 + 9*2;
		type Entry = Entry;
		const ENTRY_COUNT: usize = 512;
	}

	impl ImplementedLevel for L2 {
		const MASK: usize = 0o777_000_0000;
		const SHIFT: usize = 12 + 9*1;
		type Entry = Entry;
		const ENTRY_COUNT: usize = 512;
	}

	impl ImplementedLevel for L1 {
		const MASK: usize = 0o777_0000;
		const SHIFT: usize = 12 + 9*0;
		type Entry = Entry;
		const ENTRY_COUNT: usize = 512;
	}

	#[derive(Copy, Clone, Eq, PartialEq)]
	#[repr(transparent)]
	pub struct Entry(u64);

	bitflags! {
		impl Entry: u64 {
			const PRESENT = 1<<0;
			const ADDRESS = 0x0fff_ffff_ffff_f000;
		}
	}

	impl ImplementedEntry for Entry {
		fn empty() -> Self {
			<Self as Flags>::empty()
		}

		fn is_present(self) -> bool { self.contains(Self::PRESENT) }

		fn pointed_frame(self) -> Option<Frame> {
			if !self.is_present() { return None; }

			let addr = self.0 & Self::ADDRESS.0;
			Some(Frame::new(PhysicalAddress::new(addr.try_into().unwrap())))
		}

		fn point_to_frame(&mut self, frame: Frame) -> Result<(), ()> {
			if self.is_present() { return Err(()); }

			let empty_entry = self.0 & !Self::ADDRESS.0;
			let masked_addr = u64::try_from(frame.start().addr).unwrap() & Self::ADDRESS.0;
			self.0 = empty_entry | masked_addr | Self::PRESENT.0;

			Ok(())
		}
	}
}

pub trait PageIndices {
	fn l4_index(self) -> usize;
	fn l3_index(self) -> usize;
	fn l2_index(self) -> usize;
	fn l1_index(self) -> usize;
}

impl PageIndices for Page {
	fn l4_index(self) -> usize {
		(self.start().addr & L4::MASK) >> L4::SHIFT
	}

	fn l3_index(self) -> usize {
		(self.start().addr & L3::MASK) >> L3::SHIFT
	}

	fn l2_index(self) -> usize {
		(self.start().addr & L2::MASK) >> L2::SHIFT
	}

	fn l1_index(self) -> usize {
		(self.start().addr & L1::MASK) >> L1::SHIFT
	}
}

#[repr(C)]
pub struct Table<L: ImplementedLevel> where [(); L::ENTRY_COUNT]: {
	pub entries: [L::Entry; L::ENTRY_COUNT]
}

impl<L: ImplementedLevel> Table<L> where [(); L::ENTRY_COUNT]: {
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

impl<L: ImplementedLevel + ParentLevel> Table<L> where L::Child: ImplementedLevel, [(); L::ENTRY_COUNT]:, [(); <L::Child as ImplementedLevel>::ENTRY_COUNT]: {
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
