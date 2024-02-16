use core::arch::asm;
use core::fmt::{Debug, Formatter};
use kernel_api::bridge::paging::MapPageError;
use kernel_api::memory::allocator::{BackingAllocator};
use kernel_api::memory::{Frame, Page, PhysicalAddress, AllocError};
use kernel_api::memory::physical::highmem;
use table::{Table, PDPT, PML4, PageIndices};
use crate::paging2::{KTable, TTable};
use crate::paging::Entry;

mod table;

pub(crate) unsafe fn construct_tables() -> (Amd64KTable, Amd64TTable) {
	let ttable_base: usize;
	unsafe {
		asm!(
			"mov {}, cr3",
		out(reg) ttable_base)
	};
	let ttable_base = ttable_base & 0xffff_ffff_ffff_f000;

	let ttable = unsafe { Amd64TTable::new_unchecked(Frame::new(PhysicalAddress::new(ttable_base))) };

	let ktable_base = ttable.pml4.pml4().entries[256].pointed_frame()
			.expect("Invalid TTable");
	let ktable = Amd64KTable {
		tables: KTablePtr(ktable_base),
		allocator: highmem()
	};

	(ktable, ttable)
}

#[derive(Debug)]
struct KTablePtr(Frame); // points to a [Table<PDPT>; 256]

#[repr(align(8))]
pub struct Amd64KTable {
	tables: KTablePtr, // points to a [Table<PDPT>; 256]
	allocator: &'static dyn BackingAllocator,
}

impl KTablePtr {
	fn tables(&self) -> &[Table<PDPT>; 256] {
		unsafe {
			&*self.0.to_page().as_ptr().cast()
		}
	}

	fn tables_mut(&mut self) -> &mut [Table<PDPT>; 256] {
		unsafe {
			&mut *self.0.to_page().as_ptr().cast()
		}
	}
}

impl Debug for Amd64KTable {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		Debug::fmt(&self.tables, f)
	}
}

#[repr(transparent)]
pub(super) struct TTablePtr(pub(super) Frame); // points to a Table<PML4>

impl TTablePtr {
	fn pml4(&self) -> &Table<PML4> {
		unsafe {
			&*self.0.to_page().as_ptr().cast()
		}
	}

	fn pml4_mut(&mut self) -> &mut Table<PML4> {
		unsafe {
			&mut *self.0.to_page().as_ptr().cast()
		}
	}
}

pub struct Amd64TTable {
	pub(super) pml4: TTablePtr,
	allocator: &'static dyn BackingAllocator,
}

impl Amd64TTable {
	pub unsafe fn new_unchecked(pml4: Frame) -> Self {
		Self {
			pml4: TTablePtr(pml4),
			allocator: highmem()
		}
	}
}

impl Debug for Amd64TTable {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		Debug::fmt(self.pml4.pml4(), f)
	}
}

impl KTable for Amd64TTable {
	fn translate_page(&self, page: Page) -> Option<Frame> {
		assert!(page.start().addr < 0xffff_8000_0000_0000, "TTable only handles lower half addresses");

		let pdpt = self.pml4.pml4().child_table(page.pml4_index())?;
		let pd = pdpt.child_table(page.pdpt_index())?;
		let pt = pd.child_table(page.pd_index())?;
		pt.entries[page.pt_index()].pointed_frame()
	}

	fn map_page(&mut self, page: Page, frame: Frame) -> Result<(), MapPageError> {
		assert!(page.start().addr < 0xffff_8000_0000_0000, "TTable only handles lower half addresses");

		let pdpt = self.pml4.pml4_mut().child_table_or_new(page.pml4_index(), &self.allocator)?;
		let pd = pdpt.child_table_or_new(page.pdpt_index(), &self.allocator)?;
		let pt = pd.child_table_or_new(page.pd_index(), &self.allocator)?;
		pt.entries[page.pt_index()].point_to_frame(frame).map_err(|_| MapPageError::AlreadyMapped)
	}
}

impl KTable for Amd64KTable {
	fn translate_page(&self, page: Page) -> Option<Frame> {
		assert!(page.start().addr < 0xffff_8000_0000_0000, "TTable only handles lower half addresses");

		let pdpt = &self.tables.tables()[page.pml4_index() - 256];
		let pd = pdpt.child_table(page.pdpt_index())?;
		let pt = pd.child_table(page.pd_index())?;
		pt.entries[page.pt_index()].pointed_frame()
	}

	fn map_page(&mut self, page: Page, frame: Frame) -> Result<(), MapPageError> {
		assert!(page.start().addr >= 0xffff_8000_0000_0000, "KTable only handles upper half addresses");

		let pdpt = &mut self.tables.tables_mut()[page.pml4_index() - 256];
		let pd = pdpt.child_table_or_new(page.pdpt_index(), &self.allocator)?;
		let pt = pd.child_table_or_new(page.pd_index(), &self.allocator)?;
		pt.entries[page.pt_index()].point_to_frame(frame).map_err(|_| MapPageError::AlreadyMapped)
	}
}

impl TTable for Amd64TTable {
	type KTableTy = Amd64KTable;

	unsafe fn load(&self) {
		let addr = self.pml4.0.start().addr;
		unsafe { asm!("mov cr3, {}", in(reg) addr); }
	}

	fn new(ktable: &Amd64KTable, allocator: &'static dyn BackingAllocator) -> Result<Self, AllocError> {
		let pml4_frame = Table::<PML4>::empty_with(allocator)?;
		let pml4 = pml4_frame.to_page().as_ptr().cast::<Table<PML4>>();
		assert!(!pml4.is_null() && pml4.is_aligned());
		let pml4 = unsafe { &mut *pml4 };

		for (i, entry) in pml4.entries[256..].iter_mut().enumerate() {
			let ktable_frame = ktable.tables.0 + i;
			entry.point_to_frame(ktable_frame)
					.expect("Empty table should have no mappings");
		}

		Ok(Self {
			pml4: TTablePtr(pml4_frame),
			allocator
		})
	}
}
