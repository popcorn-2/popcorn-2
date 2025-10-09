
use core::ops::DerefMut;
use kernel_api::sync::{OnceLock, Spinlock};

use crate::hal::KTableTy;

static KERNEL_PAGE_TABLE: OnceLock<Spinlock<KTableTy>> = OnceLock::new();

pub unsafe fn init_page_table(active_page_table: KTableTy) {
	KERNEL_PAGE_TABLE.get_or_init(|| Spinlock::new(active_page_table));
}

pub fn ktable() -> impl DerefMut<Target = KTableTy> {
	KERNEL_PAGE_TABLE.get().expect("ktable not initialized").lock()
}

#[cfg(test)]
mod tests {
	use crate::hal::paging2::TTable;
	use crate::hal::TTableTy;
	use crate::memory::physical::highmem;
	use super::*;

	#[test]
	fn unmapped_page_doesnt_translate() {
		let table = TTableTy::new(&*KERNEL_PAGE_TABLE.read(), highmem()).unwrap();
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0xcafebabe000))), None);
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0xdeadbeef000))), None);
		assert_eq!(table.translate_page(Page::new(VirtualAddress::new(0x347e40000))), None);
	}

	#[test]
	fn unmapped_address_doesnt_translate() {
		let table = TTableTy::new(&*KERNEL_PAGE_TABLE.read(), highmem()).unwrap();
		assert_eq!(table.translate_address(VirtualAddress::new(0xcafebabe)), None);
		assert_eq!(table.translate_address(VirtualAddress::new(0xdeadbeef)), None);
		assert_eq!(table.translate_address(VirtualAddress::new(0x347e40)), None);
	}

	#[test]
	fn translations_after_mapping() {
		let mut table = TTableTy::new(&*KERNEL_PAGE_TABLE.read(), highmem()).unwrap();
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0x347e40000)),
			0
		).expect("Page not yet mapped");
		assert_eq!(
			table.translate_page(Page::new(VirtualAddress::new(0xcafebabe000))),
			Some(Frame::new(PhysicalAddress::new(0x347e40000)))
		);
	}

	#[test]
	fn cannot_overmap() {
		let mut table = TTableTy::new(&*KERNEL_PAGE_TABLE.read(), highmem()).unwrap();
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0x347e40000)),
			0
		).expect("Page not yet mapped");
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0xcafebabe000)),
			0
		).expect_err("Page already mapped");
	}

	#[test]
	fn address_offset() {
		let mut table = TTableTy::new(&*KERNEL_PAGE_TABLE.read(), highmem()).unwrap();
		table.map_page(
			Page::new(VirtualAddress::new(0xcafebabe000)),
			Frame::new(PhysicalAddress::new(0x347e40000)),
			0
		).expect("Page not yet mapped");
		assert_eq!(
			table.translate_address(VirtualAddress::new(0xcafebabe123)),
			Some(PhysicalAddress::new(0x347e40123))
		)
	}
}
