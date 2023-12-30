use core::cell::RefCell;
use core::fmt::{Debug, Formatter};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use kernel_api::memory::allocator::{AllocError, BackingAllocator};

use kernel_hal::paging::{Table, PageIndices, levels::Global, Entry, TableDebug};
use kernel_hal::paging::levels::ParentLevel;
use crate::sync::late_init::LateInit;

static ACTIVE_PAGE_TABLE: LateInit<ActivePageTable> = LateInit::new();

pub unsafe fn init_page_table(active_page_table: PageTable) {
	ACTIVE_PAGE_TABLE.init_ref(ActivePageTable {
		table: RefCell::new(active_page_table)
	});
}

pub fn current_page_table() -> impl DerefMut<Target = PageTable> {
	ACTIVE_PAGE_TABLE.table.borrow_mut()
}

/*
FIXME: This entire struct should be used as a `thread_local` and so lack of Sync is fine
 However, SMP is currently not supported so we only have one active page table and one "thread"
 This is possibly unsound because of interrupts, but the RefCell here will attempt to catch any
 misuse
 Under amd64, which this is mostly tested using, this should maybe work fine because of always
 strong memory ordering?
 */
struct ActivePageTable {
	table: RefCell<PageTable>
}
unsafe impl Sync for ActivePageTable {}

pub struct PageTable {
	l4: NonNull<Table<Global>>
}

impl PageTable {
	pub unsafe fn new_unchecked(frame: Frame) -> Self {
		Self {
			l4: NonNull::new_unchecked(frame.to_page().as_ptr().cast())
		}
	}

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

	pub fn map_page(&mut self, page: Page, frame: Frame, allocator: impl BackingAllocator) -> Result<(), MapPageError> {
		let upper_table = unsafe { self.l4.as_mut() }.child_table_or_new(page.global_index(), &allocator)?;
		let middle_table = upper_table.child_table_or_new(page.upper_index(), &allocator)?;
		let lower_table = middle_table.child_table_or_new(page.middle_index(), &allocator)?;
		lower_table.entries[page.lower_index()].point_to_frame(frame).map_err(|_| MapPageError::AlreadyMapped)
	}
}

impl Debug for PageTable {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		unsafe { self.l4.as_ref() }.debug_fmt(f, 0)
	}
}

#[derive(Debug, Copy, Clone)]
pub enum MapPageError {
	AllocError,
	AlreadyMapped
}

#[doc(hidden)]
impl From<AllocError> for MapPageError {
	fn from(_value: AllocError) -> Self {
		Self::AllocError
	}
}

mod mapping {
	use kernel_api::memory::allocator::{AllocError, BackingAllocator};
	use kernel_api::memory::{Frame, Page};
	use kernel_api::memory::r#virtual::VirtualAllocator;
	use crate::memory::paging::{current_page_table, PageTable};
	use crate::memory::physical::highmem;
	use crate::memory::r#virtual::Global;

	pub struct Mapping {
		base: Page,
		len: usize
	}

	impl Mapping {
		pub fn new(len: usize) -> Result<Self, AllocError> {
			let highmem_lock = highmem();
			Self::new_with(len, &*highmem_lock)
		}

		pub fn new_with(len: usize, physical_allocator: impl BackingAllocator) -> Result<Self, AllocError> {
			// FIXME: memory leak here on error from lack of ArcFrame
			let physical_mem = physical_allocator.allocate_contiguous(len)?;
			let virtual_mem = Global.allocate_contiguous(len)?;

			// TODO: huge pages
			let mut page_table = current_page_table();
			for (frame, page) in (0..len).map(|i| (physical_mem + i, virtual_mem + i)) {
				page_table.map_page(page, frame, &physical_allocator).expect("todo");
			}

			Ok(Self {
				base: virtual_mem,
				len
			})
		}

		pub fn remap(&mut self, new_len: usize) -> Result<(), AllocError> {
			if new_len == self.len { return Ok(()); }

			let original_physical_allocator: &dyn BackingAllocator = todo!("retrieve original allocator");
			let physical_base: Frame = todo!("translate base to locate physical backing");

			if new_len < self.len {
				// todo: actually free and unmap the extra memory

				self.len = new_len;
				Ok(())
			} else {
				let extra_len = new_len - self.len;

				let extra_physical_mem = original_physical_allocator.allocate_contiguous(extra_len)?;
				let extra_virtual_mem = Global.allocate_contiguous_at(self.base + self.len, extra_len);

				match extra_virtual_mem {
					Ok(_) => {
						let start_of_extra = self.base + self.len;

						// TODO: huge pages
						let mut page_table = current_page_table();

						for (frame, page) in (0..extra_len).map(|i| (extra_physical_mem + i, start_of_extra + i)) {
							page_table.map_page(page, frame, original_physical_allocator).expect("todo");
						}

						self.len = new_len;
						Ok(())
					}
					Err(_) => {
						let new_virtual_mem = Global.allocate_contiguous(new_len)?;

						let mut page_table = current_page_table();

						for (frame, page) in (0..self.len).map(|i| (physical_base + i, new_virtual_mem + i)) {
							page_table.map_page(page, frame, original_physical_allocator).expect("todo");
						}
						for (frame, page) in (0..extra_len).map(|i| (extra_physical_mem + i, new_virtual_mem + self.len + i)) {
							page_table.map_page(page, frame, original_physical_allocator).expect("todo");
						}

						self.base = new_virtual_mem;
						self.len = new_len;

						Ok(())
					}
				}
			}
		}

		pub fn into_raw_parts(self) -> (Page, usize) {
			let Self { base, len, .. } = self;
			(base, len)
		}

		pub unsafe fn from_raw_parts(base: Page, len: usize) -> Self {
			Self {
				base,
				len
			}
		}
	}

	impl Drop for Mapping {
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
