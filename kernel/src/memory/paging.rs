use core::ptr::NonNull;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use kernel_api::memory::allocator::{AllocError, BackingAllocator};

use kernel_hal::paging::{Table, PageIndices, levels::Global, Entry};

pub struct PageTable {
	l4: NonNull<Table<Global>>
}

impl PageTable {
	fn empty(allocator: impl BackingAllocator) -> Result<Self, AllocError> {
		Ok(PageTable {
			l4: Table::empty_with(allocator)?.1
		})
	}

	fn translate_page(&self, page: Page) -> Option<Frame> {
		let upper_table = unsafe { self.l4.as_ref() }.child_table(page.global_index())?;
		let middle_table = upper_table.child_table(page.upper_index())?;
		let lower_table = middle_table.child_table(page.middle_index())?;
		lower_table.entries[page.lower_index()].pointed_frame()
	}

	fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
		let aligned = addr.align_down();
		let diff = addr - aligned;
		let physical = self.translate_page(Page::new(aligned))?;
		Some(physical.start() + diff)
	}

	fn map_page(&mut self, page: Page, frame: Frame, allocator: impl BackingAllocator) -> Result<(), MapPageError> {
		let upper_table = unsafe { self.l4.as_mut() }.child_table_or_new(page.global_index(), &allocator)?;
		let middle_table = upper_table.child_table_or_new(page.upper_index(), &allocator)?;
		let lower_table = middle_table.child_table_or_new(page.middle_index(), &allocator)?;
		lower_table.entries[page.lower_index()].point_to_frame(frame).map_err(|_| MapPageError::AlreadyMapped)
	}
}

#[derive(Debug, Copy, Clone)]
enum MapPageError {
	AllocError,
	AlreadyMapped
}

impl From<AllocError> for MapPageError {
	fn from(_value: AllocError) -> Self {
		Self::AllocError
	}
}

mod mapping {
	use kernel_api::memory::allocator::{AllocError, BackingAllocator};
	use kernel_api::memory::{Frame, Page};
	use kernel_api::memory::r#virtual::VirtualAllocator;
	use crate::memory::paging::PageTable;
	use crate::memory::physical::highmem;

	pub struct Global;

	impl VirtualAllocator for Global {

	}

	pub struct Mapping<A: VirtualAllocator = Global> {
		base: Page,
		len: usize,
		allocator: A
	}

	impl Mapping<Global> {
		fn new(len: usize) -> Result<Self, AllocError> {
			let highmem_lock = highmem();
			Self::new_with(len, &*highmem_lock)
		}

		fn new_with(len: usize, physical_allocator: impl BackingAllocator) -> Result<Self, AllocError> {
			// FIXME: memory leak here from lack of ArcFrame
			let physical = physical_allocator.allocate_contiguous(len)?;
			let r#virtual: Page = {  }.allocate_contiguous(len)?;

			// TODO: huge pages
			let page_table: &mut PageTable = {  };
			for (frame, page) in (0..len).map(|i| (physical + i, r#virtual + i)) {
				page_table.map_page(page, frame, &physical_allocator).expect("todo");
			}

			Ok(Self {
				base: r#virtual,
				len,
				allocator: Global
			})
		}
	}

	impl<A: VirtualAllocator> Drop for Mapping<A> {
		fn drop(&mut self) {
			// todo
		}
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
			Frame::new(PhysicalAddress::new(0x347e40000)),
			&*highmem()
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
			Frame::new(PhysicalAddress::new(0x347e40000)),
			&*highmem()
		).expect("Page not yet mapped");
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0xcafebabe000)),
			&*highmem()
		).expect_err("Page already mapped");
	}

	#[test]
	fn address_offset() {
		let mut table = PageTable::empty(&*highmem()).unwrap();
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0x347e40000)),
			&*highmem()
		).expect("Page not yet mapped");
		assert_eq!(
			table.translate_address(VirtualAddress::new(0xcafebabe123)),
			Some(PhysicalAddress::new(0x347e40123))
		)
	}
}
