use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use kernel_api::memory::allocator::{AllocError, BackingAllocator};

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

pub struct PageTable {
	l4: Frame
}

impl PageTable {
	fn empty(allocator: impl BackingAllocator) -> Result<Self, AllocError> {
		Ok(PageTable {
			l4: allocator.allocate_one()?
		})
	}

	fn translate_page(&self, page: Page) -> Option<Frame> {
		todo!()
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
