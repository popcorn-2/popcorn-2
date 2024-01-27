use core::arch::asm;
use core::fmt::{Debug, Formatter};
use kernel_api::bridge::paging::MapPageError;
use kernel_api::memory::allocator::{AllocError, BackingAllocator};
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use table::{Table, PDPT, PML4, PageIndices};
use crate::paging::Entry;

mod table;

pub(crate) unsafe fn construct_tables() -> (KTable, TTable) {
	let ttable_base: usize;
	unsafe {
		asm!(
			"mov {}, cr3",
		out(reg) ttable_base)
	};
	let ttable_base = ttable_base & 0xffff_ffff_ffff_f000;

	let ttable = unsafe { TTable::new_unchecked(Frame::new(PhysicalAddress::new(ttable_base))) };

	let ktable_base = ttable.pml4().entries[256].pointed_frame()
			.expect("Invalid TTable");
	let ktable = KTable {
		tables: ktable_base
	};

	(ktable, ttable)
}

#[derive(Debug)]
pub struct KTable {
	tables: Frame, // points to a [Table<PDPT>; 256]
}

impl KTable {
	fn tables(&self) -> &[Table<PDPT>; 256] {
		unsafe {
			&*self.tables.to_page().as_ptr().cast()
		}
	}
}

#[repr(transparent)]
pub struct TTable {
	pml4: Frame, // points to a Table<PML4>
}

impl TTable {
	pub unsafe fn new_unchecked(pml4: Frame) -> Self {
		Self {
			pml4
		}
	}

	pub fn new(ktable: &KTable, allocator: impl BackingAllocator) -> Result<Self, AllocError> {
		let pml4_frame = Table::<PML4>::empty_with(allocator)?;
		let pml4 = pml4_frame.to_page().as_ptr().cast::<Table<PML4>>();
		assert!(!pml4.is_null() && pml4.is_aligned());
		let pml4 = unsafe { &mut *pml4 };

		for (i, entry) in pml4.entries[256..].iter_mut().enumerate() {
			let ktable_frame = ktable.tables + i;
			entry.point_to_frame(ktable_frame)
					.expect("Empty table should have no mappings");
		}

		Ok(Self {
			pml4: pml4_frame,
		})
	}

	fn pml4(&self) -> &Table<PML4> {
		unsafe {
			&*self.pml4.to_page().as_ptr().cast()
		}
	}

	fn pml4_mut(&mut self) -> &mut Table<PML4> {
		unsafe {
			&mut *self.pml4.to_page().as_ptr().cast()
		}
	}
}

impl Debug for TTable {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		Debug::fmt(self.pml4(), f)
	}
}

trait PageTable {
	fn translate_page(&self, page: Page) -> Option<Frame>;

	fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
		let aligned = addr.align_down();
		let diff = addr - aligned;
		let physical = self.translate_page(Page::new(aligned))?;
		Some(physical.start() + diff)
	}

	fn map_page(&mut self, page: Page, frame: Frame, allocator: impl BackingAllocator) -> Result<(), MapPageError>;
}

impl PageTable for TTable {
	fn translate_page(&self, page: Page) -> Option<Frame> {
		assert!(page.start().addr < 0xffff_8000_0000_0000, "TTable only handles lower half addresses");

		let pdpt = self.pml4().child_table(page.pml4_index())?;
		let pd = pdpt.child_table(page.pdpt_index())?;
		let pt = pd.child_table(page.pd_index())?;
		pt.entries[page.pt_index()].pointed_frame()
	}

	fn map_page(&mut self, page: Page, frame: Frame, allocator: impl BackingAllocator) -> Result<(), MapPageError> {
		assert!(page.start().addr < 0xffff_8000_0000_0000, "TTable only handles lower half addresses");

		let pdpt = self.pml4_mut().child_table_or_new(page.pml4_index(), &allocator)?;
		let pd = pdpt.child_table_or_new(page.pdpt_index(), &allocator)?;
		let pt = pd.child_table_or_new(page.pd_index(), &allocator)?;
		pt.entries[page.pt_index()].point_to_frame(frame).map_err(|_| MapPageError::AlreadyMapped)
	}
}
