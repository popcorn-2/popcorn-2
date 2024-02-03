use core::cell::RefCell;
use core::fmt::{Debug, Formatter};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use kernel_api::memory::allocator::{AllocError, BackingAllocator};
use kernel_api::sync::RwLock;

use kernel_hal::paging::{Table, PageIndices, levels::Global, Entry, TableDebug};
use kernel_hal::paging2::{KTable, KTableTy};
use kernel_hal::paging::levels::ParentLevel;
use crate::sync::late_init::LateInit;

static KERNEL_PAGE_TABLE: LateInit<RwLock<KTableTy>> = LateInit::new();

pub unsafe fn init_page_table(active_page_table: KTableTy) {
	KERNEL_PAGE_TABLE.init_ref(RwLock::new(active_page_table));
}

#[export_name = "__popcorn_paging_get_ktable"]
pub fn ktable() -> impl DerefMut<Target = impl KTable> {
	KERNEL_PAGE_TABLE.write()
}

#[cfg(test)]
mod tests {
	use crate::memory::physical::highmem;
	use super::*;

	#[cfg(any())]
	#[test]
	fn unmapped_page_doesnt_translate() {
		let table = PageTable::empty(&*highmem()).unwrap();
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0xcafebabe000))), None);
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0xdeadbeef000))), None);
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0x347e40000))), None);
	}

	#[cfg(any())]
	#[test]
	fn unmapped_address_doesnt_translate() {
		let table = PageTable::empty(&*highmem()).unwrap();
		assert_eq!(table.translate_address(VirtualAddress::new(0xcafebabe)), None);
		assert_eq!(table.translate_address(VirtualAddress::new(0xdeadbeef)), None);
		assert_eq!(table.translate_address(VirtualAddress::new(0x347e40)), None);
	}

	#[cfg(any())]
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

	#[cfg(any())]
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

	#[cfg(any())]
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
