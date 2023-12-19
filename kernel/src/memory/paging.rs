use core::marker::PhantomData;
use core::ptr::NonNull;
use bitflags::bitflags;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use kernel_api::memory::allocator::{AllocError, BackingAllocator};

#[allow(clippy::unusual_byte_groupings)]
mod amd64 {
	pub const L4_SHIFT: usize = 12 + 9*3;
	pub const L3_SHIFT: usize = 12 + 9*2;
	pub const L2_SHIFT: usize = 12 + 9*1;
	pub const L1_SHIFT: usize = 12;
	pub const L4_MASK:  usize = 0o777_000_000_000_0000;
	pub const L3_MASK:  usize =     0o777_000_000_0000;
	pub const L2_MASK:  usize =         0o777_000_0000;
	pub const L1_MASK:  usize =             0o777_0000;
}

trait PageIndices {
	fn l4_index(self) -> usize;
	fn l3_index(self) -> usize;
	fn l2_index(self) -> usize;
	fn l1_index(self) -> usize;
}

impl PageIndices for Page {
	fn l4_index(self) -> usize {
		(self.start().addr & amd64::L4_MASK) >> amd64::L4_SHIFT
	}

	fn l3_index(self) -> usize {
		(self.start().addr & amd64::L3_MASK) >> amd64::L3_SHIFT
	}

	fn l2_index(self) -> usize {
		(self.start().addr & amd64::L2_MASK) >> amd64::L2_SHIFT
	}

	fn l1_index(self) -> usize {
		(self.start().addr & amd64::L1_MASK) >> amd64::L1_SHIFT
	}
}

enum L4 {}
enum L3 {}
enum L2 {}
enum L1 {}

trait Level {}
trait ParentLevel: Level {
	type Child: Level;
}

impl Level for L4 {}
impl Level for L3 {}
impl Level for L2 {}
impl Level for L1 {}

impl ParentLevel for L4 {
	type Child = L3;
}
impl ParentLevel for L3 {
	type Child = L2;
}
impl ParentLevel for L2 {
	type Child = L1;
}

#[repr(C)]
struct Table<L> {
	entries: [Entry; 512],
	level: PhantomData<L>
}

impl<L> Table<L> {
	fn empty() -> Self {
		Self {
			entries: [Entry::empty(); 512],
			level: PhantomData
		}
	}

	fn empty_with(allocator: impl BackingAllocator) -> Result<NonNull<Self>, AllocError> {
		let table_frame = allocator.allocate_one()?;
		let table_ptr: *mut Self = table_frame.to_page().as_ptr().cast();

		unsafe { table_ptr.write(Table::empty()); }

		Ok(NonNull::new(table_ptr).expect("Just allocated this pointer"))
	}
}

impl<L: ParentLevel> Table<L> {
	fn child_table(&self, idx: usize) -> Option<&Table<L::Child>> {
		let entry = self.entries[idx];
		let table_frame = entry.pointed_frame()?;
		let table_page = table_frame.to_page();
		Some(unsafe { &*table_page.as_ptr().cast() })
	}

	fn child_table_mut(&mut self, idx: usize) -> Option<&mut Table<L::Child>> {
		let entry = self.entries[idx];
		let table_frame = entry.pointed_frame()?;
		let table_page = table_frame.to_page();
		Some(unsafe { &mut *table_page.as_ptr().cast() })
	}
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(transparent)]
struct Entry(u64);

bitflags! {
	impl Entry: u64 {
		const PRESENT = 1<<0;
		const ADDRESS = 0x0fff_ffff_ffff_f000;
	}
}

impl Entry {
	fn is_present(self) -> bool { self.contains(Self::PRESENT) }

	fn pointed_frame(self) -> Option<Frame> {
		if !self.is_present() { return None; }

		let addr = self.0 & Self::ADDRESS.0;
		Some(Frame::new(PhysicalAddress::new(addr.try_into().unwrap())))
	}
}

pub struct PageTable {
	l4: NonNull<Table<L4>>
}

impl PageTable {
	fn empty(allocator: impl BackingAllocator) -> Result<Self, AllocError> {
		Ok(PageTable {
			l4: Table::empty_with(allocator)?
		})
	}

	fn translate_page(&self, page: Page) -> Option<Frame> {
		let l3_table = unsafe { self.l4.as_ref() }.child_table(page.l4_index())?;
		let l2_table = l3_table.child_table(page.l3_index())?;
		let l1_table = l2_table.child_table(page.l2_index())?;
		l1_table.entries[page.l1_index()].pointed_frame()
	}

	fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
		let aligned = addr.align_down();
		let diff = addr - aligned;
		let physical = self.translate_page(Page::new(aligned))?;
		Some(physical.start() + diff)
	}

	fn map_page(&mut self, page: Page, frame: Frame) -> Result<(), ()> {
		todo!()
	}
}

#[cfg(test)]
mod tests {
	use crate::memory::physical::highmem;
	use super::*;

	#[test]
	fn unmapped_page_doesnt_translate() {
		let table = PageTable::empty(&*highmem()).unwrap();
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0xcafebabe000))), None);
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0xdeadbeef000))), None);
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0x347e40000))), None);
	}

	#[test]
	fn unmapped_address_doesnt_translate() {
		let table = PageTable::empty(&*highmem()).unwrap();
		assert_eq!(table.translate_address(VirtualAddress::new(0xcafebabe)), None);
		assert_eq!(table.translate_address(VirtualAddress::new(0xdeadbeef)), None);
		assert_eq!(table.translate_address(VirtualAddress::new(0x347e40)), None);
	}

	#[test]
	fn translations_after_mapping() {
		let mut table = PageTable::empty(&*highmem()).unwrap();
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0x347e40000))
		).expect("Page not yet mapped");
		assert_eq!(
			table.translate_page(Page::new(VirtualAddress::new(0xcafebabe000))),
			Some(Frame::new(PhysicalAddress::new(0x347e40000)))
		);
	}

	#[test]
	fn cannot_overmap() {
		let mut table = PageTable::empty(&*highmem()).unwrap();
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0x347e40000))
		).expect("Page not yet mapped");
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0xcafebabe000))
		).expect_err("Page already mapped");
	}

	#[test]
	fn address_offset() {
		let mut table = PageTable::empty(&*highmem()).unwrap();
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0x347e40000))
		).expect("Page not yet mapped");
		assert_eq!(
			table.translate_address(VirtualAddress::new(0xcafebabe123)),
			Some(PhysicalAddress::new(0x347e40123))
		)
	}
}
