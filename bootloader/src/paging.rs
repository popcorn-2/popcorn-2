use bitflags::{bitflags, Flags};
use core::arch::asm;
use core::fmt;
use core::marker::PhantomData;
use core::mem::{MaybeUninit, size_of};
use core::ops;
use core::ptr::addr_of;
use kernel_exports::memory::{PhysicalAddress, VirtualAddress};
use log::debug;
use uefi::table::boot::PAGE_SIZE;

#[derive(Debug, Copy, Clone)]
pub struct Page(pub u64);

impl Page {
	const fn l4_index(self) -> u64 { (self.0 & amd64::L4_MASK) >> amd64::L4_SHIFT }
	const fn l3_index(self) -> u64 { (self.0 & amd64::L3_MASK) >> amd64::L3_SHIFT }
	const fn l2_index(self) -> u64 { (self.0 & amd64::L2_MASK) >> amd64::L2_SHIFT }
	const fn l1_index(self) -> u64 { (self.0 & amd64::L1_MASK) >> amd64::L1_SHIFT }
}

#[derive(Debug, Copy, Clone)]
pub struct Frame(pub u64);

pub struct PageTable(&'static mut Table<Level4>);

impl PageTable {
	pub unsafe fn try_new<E, F: FnOnce() -> Result<u64, E>>(allocate: F) -> Result<PageTable, E> {
		let table_ptr = allocate()? as *mut MaybeUninit<Table<Level4>>;
		assert!(table_ptr.is_aligned() && !table_ptr.is_null());
		let table = &mut *table_ptr;
		let table = table.write(Table::new());

		Ok(PageTable(table))
	}

	pub fn try_map_page<E, F: Fn() -> Result<u64, E>>(&mut self, page: Page, frame: Frame, allocate: F) -> Result<(),MapError<E>> {
		self.try_map_page_with(page, frame, allocate, TableEntryFlags::empty())
	}

	pub fn try_map_page_with<E, F: Fn() -> Result<u64, E>>(&mut self, page: Page, frame: Frame, allocate: F, flags: TableEntryFlags) -> Result<(),MapError<E>> {
		let entry = &mut self.0.try_get_or_create_child_table(page.l4_index().try_into().unwrap(), &allocate)?
			.try_get_or_create_child_table(page.l3_index().try_into().unwrap(), &allocate)?
			.try_get_or_create_child_table(page.l2_index().try_into().unwrap(), &allocate)?
			[page.l1_index().try_into().unwrap()];
		entry.set_pointed_frame(frame, flags).map_err(|_| MapError::AlreadyMapped)
	}

	pub fn switch(&self) {
		let addr = self.0 as *const _ as usize;
		unsafe{ asm!("mov cr3, {}", in(reg) addr, options(nostack, preserves_flags)); }
	}

	fn table_addr(&self) -> usize {
		addr_of!(*self.0) as usize
	}
}

impl fmt::Pointer for PageTable {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		if f.alternate() {
			writeln!(f, "{:#01$x}", self.table_addr(), size_of::<*mut u8>()*2 + 2)
		} else {
			writeln!(f, "{:#x}", self.table_addr())
		}
	}
}

impl From<&PageTable> for kernel_exports::memory::Frame {
	fn from(value: &PageTable) -> Self {
		unsafe {
			kernel_exports::memory::Frame::new_unchecked(
				kernel_exports::memory::PhysicalAddress(value.table_addr())
			)
		}
	}
}

#[derive(Debug, Copy, Clone)]
pub enum MapError<E> {
	AlreadyMapped,
	AllocationError(E)
}

impl<E> From<E> for MapError<E> {
	fn from(value: E) -> Self {
		Self::AllocationError(value)
	}
}

#[derive(Debug)]
#[repr(C)]
struct Table<Level: TableLevel>([TableEntry; 512], PhantomData<Level>);

impl<Level: TableLevel> Table<Level> {
	const fn new() -> Self {
		Self([const { TableEntry::new() }; 512], PhantomData)
	}
}

impl<Level: ParentTableLevel> Table<Level> {
	pub fn get_child_table(&self, index: usize) -> Option<&'static Table<Level::Child>> {
		self.0[index].pointed_frame()
				.map(|frame| unsafe { &*(frame.0 as *const Table<_>) })
	}

	pub fn get_child_table_mut(&mut self, index: usize) -> Option<&'static mut Table<Level::Child>> {
		self.0[index].pointed_frame()
		             .map(|frame| unsafe { &mut *(frame.0 as *mut Table<_>) })
	}

	pub fn try_get_or_create_child_table<E, F: FnOnce() -> Result<u64, E>>(&mut self, index: usize, allocate: F) -> Result<&'static mut Table<Level::Child>, E> {
		self.get_child_table_mut(index).map_or_else(|| {
			debug!("New page table Level {}, index {}", Level::Child::VALUE, index);
			let table_ptr = allocate()? as *mut MaybeUninit<Table<_>>;
			assert!(table_ptr.is_aligned() && !table_ptr.is_null());
			let table = unsafe { &mut *table_ptr };
			let table = table.write(Table::new());
			self.0[index].set_pointed_frame(Frame(table_ptr as u64), TableEntryFlags::PERMISSIVE).unwrap();
			Ok(table)
		}, Ok)
	}
}

impl<Level: TableLevel> ops::Index<usize> for Table<Level> {
	type Output = TableEntry;

	fn index(&self, index: usize) -> &Self::Output {
		&self.0[index]
	}
}

impl<Level: TableLevel> ops::IndexMut<usize> for Table<Level> {
	fn index_mut(&mut self, index: usize) -> &mut Self::Output {
		&mut self.0[index]
	}
}

#[derive(Debug)]
enum Level4 {}
#[derive(Debug)]
enum Level3 {}
#[derive(Debug)]
enum Level2 {}
#[derive(Debug)]
enum Level1 {}

trait TableLevel {
	const VALUE: u8;
}
impl TableLevel for Level4 { const VALUE: u8 = 4; }
impl TableLevel for Level3 { const VALUE: u8 = 3; }
impl TableLevel for Level2 { const VALUE: u8 = 2; }
impl TableLevel for Level1 { const VALUE: u8 = 1; }

trait ParentTableLevel: TableLevel {
	type Child: TableLevel;
}
impl ParentTableLevel for Level4 { type Child = Level3; }
impl ParentTableLevel for Level3 { type Child = Level2; }
impl ParentTableLevel for Level2 { type Child = Level1; }

#[repr(C)]
struct TableEntry(u64);

impl fmt::Debug for TableEntry {
	fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
		let f = self.pointed_frame();
		let flags = self.flags();
		fmt.debug_struct("TableEntry")
				.field("frame", &f)
				.field("flags", &flags)
				.finish()
	}
}

bitflags! {
	#[derive(Copy, Clone, Debug)]
	pub struct TableEntryFlags: u64 {
		const PRESENT =         1 << 0;
        const WRITABLE =        1 << 1;
        const USER_ACCESSIBLE = 1 << 2;
        const WRITE_THROUGH =   1 << 3;
        const NO_CACHE =        1 << 4;
        const ACCESSED =        1 << 5;
        const DIRTY =           1 << 6;
        const HUGE_PAGE =       1 << 7;
        const GLOBAL =          1 << 8;
        const NO_EXECUTE =      1 << 63;

		const UEFI_USED =       1 << 9;

		const PERMISSIVE =      Self::WRITABLE.bits() | Self::USER_ACCESSIBLE.bits();
		const MMIO =            Self::WRITE_THROUGH.bits() | Self::NO_CACHE.bits();
	}
}

impl TableEntry {
	const fn new() -> Self { Self(TableEntryFlags::WRITABLE.bits()) }

	fn flags(&self) -> TableEntryFlags {
		TableEntryFlags::from_bits_truncate(self.0)
	}

	fn pointed_frame(&self) -> Option<Frame> {
		if self.flags().contains(TableEntryFlags::PRESENT) {
			Some(Frame(self.0 & 0x000f_ffff_ffff_f000))
		} else { None }
	}

	fn set_pointed_frame_unchecked(&mut self, frame: Frame, flags: TableEntryFlags) {
		self.0 &= !0x000f_ffff_ffff_f000;
		self.0 |= frame.0 & 0x000f_ffff_ffff_f000;
		self.0 |= (flags | TableEntryFlags::PRESENT).bits();
	}

	fn set_pointed_frame(&mut self, frame: Frame, flags: TableEntryFlags) -> Result<(), Frame> {
		self.pointed_frame()
				.map_or_else(|| {
					self.set_pointed_frame_unchecked(frame, flags);
					Ok(())
				}, Err)
	}
}

#[allow(clippy::unusual_byte_groupings)]
mod amd64 {
	pub const L4_SHIFT: u64 = 12 + 9*3;
	pub const L3_SHIFT: u64 = 12 + 9*2;
	pub const L2_SHIFT: u64 = 12 + 9*1;
	pub const L1_SHIFT: u64 = 12;
	pub const L4_MASK:  u64 = 0o777_000_000_000_0000;
	pub const L3_MASK:  u64 =     0o777_000_000_0000;
	pub const L2_MASK:  u64 =         0o777_000_0000;
	pub const L1_MASK:  u64 =             0o777_0000;
}
