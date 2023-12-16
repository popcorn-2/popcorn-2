use core::alloc::{AllocError, Layout};
use core::fmt::{Debug, DebugMap, Formatter};
use core::hint::unreachable_unchecked;
use core::iter::Map;
use core::mem;
use core::ops::{Deref, DerefMut, Index, IndexMut};
use core::ptr::{addr_of, NonNull};
use derive_more::{Index, IndexMut};
use kernel_exports::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use kernel_exports::sync::Mutex;
use crate::memory::Allocator;
use crate::{sprintln, usize};
use crate::memory::alloc::phys;

#[derive(Debug)] enum L4 {}
#[derive(Debug)] enum L3 {}
#[derive(Debug)] enum L2 {}
#[derive(Debug)] enum L1 {}

/// Returns the [`Frame`] currently mapped on error
pub type MapError = Result<(), Frame>;

#[derive(Default, Debug)]
struct Foo(u64);

impl EntryTy for Foo {
	fn pointed_frame(&self) -> Option<Frame> {
		if self.is_present() {
			Some(Frame::align_down(PhysicalAddress((self.0 & 0x000fffff_fffff000).try_into().unwrap())))
		} else { None }
	}

	fn is_present(&self) -> bool {
		self.0 & 1 != 0
	}

	fn is_huge(&self) -> bool {
		self.0 & (1<<7) != 0
	}

	fn point_to_unchecked(&mut self, frame: Frame) {
		self.0 &= !0x000fffff_fffff000;
		self.0 |= u64::try_from(frame.start().0 & 0x000fffff_fffff000).unwrap();
		self.0 |= (1<<0) | (1<<1) | (1<<63);
	}

	fn unmap(&mut self) {
		self.0 &= !1;
	}

	fn clear(&mut self) {
		self.0 = 0;
	}
}

pub trait EntryTy: Debug {
	#[must_use]
	fn pointed_frame(&self) -> Option<Frame>;
	fn point_to(&mut self, frame: Frame) -> MapError {
		if let Some(f) = self.pointed_frame() { Err(f) }
		else {
			self.point_to_unchecked(frame);
			Ok(())
		}
	}
	fn is_present(&self) -> bool;
	fn is_huge(&self) -> bool;
	fn point_to_unchecked(&mut self, frame: Frame);
	fn unmap(&mut self);
	fn clear(&mut self);
}

pub trait Level: Debug {
	type Entry: EntryTy;

	const ADDRESS_MASK: usize;
	const ADDRESS_SHIFT: usize = {
		let trail = Self::ADDRESS_MASK.trailing_zeros() as usize;
		if trail == mem::size_of::<usize>() { 0 }
		else { trail }
	};
}
trait ParentLevel: Level {
	type Child: Level;
}

impl Level for L4 {
	type Entry = Foo;
	const ADDRESS_MASK: usize = 0o777_000_000_000_0000;
}
impl Level for L3 {
	type Entry = Foo;
	const ADDRESS_MASK: usize = 0o000_777_000_000_0000;
}
impl Level for L2 {
	type Entry = Foo;
	const ADDRESS_MASK: usize = 0o000_000_777_000_0000;
}
impl Level for L1 {
	type Entry = Foo;
	const ADDRESS_MASK: usize = 0o000_000_000_777_0000;
}

impl ParentLevel for L4 { type Child = L3; }
impl ParentLevel for L3 { type Child = L2; }
impl ParentLevel for L2 { type Child = L1; }

#[derive(Index, IndexMut, Debug)]
#[repr(C, align(4096))]
struct Table<L: Level> {
	entries: [L::Entry; 512]
}

impl<L: Level> Table<L> {
	pub fn clear(&mut self) {
		for entry in &mut self.entries {
			entry.clear();
		}
	}
}

impl<L: ParentLevel> Table<L> {
	pub fn child_table(&self, idx: usize) -> Option<&Table<L::Child>> {
		if self[idx].is_present() && !self[idx].is_huge() {
			let foo = ((usize::MAX << 48) | (self as *const _ as usize) << 9) | (idx << 12);
			Some(unsafe {
				&*(foo as *const _)
			})
		} else { None }
	}

	pub fn child_table_mut(&mut self, idx: usize) -> Option<&mut Table<L::Child>> {
		if self[idx].is_present() && !self[idx].is_huge() {
			let foo = ((usize::MAX << 48) | (self as *mut _ as usize) << 9) | (idx << 12);
			Some(unsafe {
				&mut *(foo as *mut _)
			})
		} else { None }
	}

	#[track_caller]
	pub fn child_table_create<A: Allocator>(&mut self, idx: usize, alloc: &A) -> &mut Table<L::Child> {
		if self.child_table(idx).is_none() {
			let f = alloc.allocate_one_aligned_zeroed(0).expect("Allocation failed");
			if self.entries[idx].point_to(f).is_err() { unsafe { unreachable_unchecked(); } }
		}

		match self.child_table_mut(idx) {
			Some(t) => t,
			None => unsafe { unreachable_unchecked() }
		}
	}
}

trait PageExtensions {
	fn index<L: Level>(&self) -> usize;
}

impl PageExtensions for Page  {
	fn index<L: Level>(&self) -> usize {
		(self.start().0 & L::ADDRESS_MASK) >> L::ADDRESS_SHIFT
	}
}

pub struct Mapper {
	table: NonNull<Table<L4>>
}

unsafe impl Send for Mapper {}

impl Mapper {
	fn table_mut(&mut self) -> &mut Table<L4> { unsafe { self.table.as_mut() } }
	fn table(&self) -> &Table<L4> { unsafe { self.table.as_ref() } }

	pub fn debug_mappings(&self) -> impl Debug + '_ { TableMappingDebugger(self) }
	pub fn debug_table(&self) -> impl Debug + '_ { TableTableDebugger(self.table()) }

	pub fn map_page_to<A: Allocator>(&mut self, page: Page, frame: Frame, alloc: &A) -> MapError {
		let l1 = self.table_mut()
				.child_table_create(page.index::<L4>(), alloc)
				.child_table_create(page.index::<L3>(), alloc)
				.child_table_create(page.index::<L2>(), alloc)
				.index_mut(page.index::<L1>());
		l1.point_to(frame)
	}

	pub fn unmap_page(&mut self, page: Page) {
		let l1 = self.table_mut()
		             .child_table_mut(page.index::<L4>()).unwrap()
		             .child_table_mut(page.index::<L3>()).unwrap()
		             .child_table_mut(page.index::<L2>()).unwrap()
		             .index_mut(page.index::<L1>());
		l1.unmap();
		hooks::unmap_page(page);
	}
}

pub struct ActiveTable(Mapper);

impl ActiveTable {
	const unsafe fn new() -> Self {
		Self(Mapper {
			table: NonNull::new_unchecked(0o177777_400_400_400_400_0000 as *mut Table<L4>)
		})
	}

	pub fn modify_with<R, F: FnOnce(&mut Mapper) -> R>(&mut self, table: &mut InactiveTable, f: F) -> R {
		let self_frame = self.table_mut().entries[256].pointed_frame().expect("Page table was not recursive");
		self.table_mut().entries[257].point_to(self_frame).expect("Inactive page table modification was already happening");
		self.table_mut().entries[256].point_to_unchecked(table.frame);
		let ret = f(&mut **self);
		unsafe { (&mut *(0o177777_401_401_401_401_4000 as *mut <L4 as Level>::Entry)).point_to_unchecked(self_frame) };
		ret
	}
}

impl Deref for ActiveTable {
	type Target = Mapper;

	fn deref(&self) -> &Self::Target { &self.0 }
}

impl DerefMut for ActiveTable {
	fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

pub struct TableMappingDebugger<'table>(&'table Mapper);
pub struct TableTableDebugger<'table, L: Level>(&'table Table<L>);

impl<'a> Debug for TableMappingDebugger<'a> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		trait TablePrint { fn print_to(&self, d: &mut DebugMap, addr: usize); }

		impl<L: ParentLevel> TablePrint for Table<L> {
			fn print_to(&self, d: &mut DebugMap, addr: usize) {
				for i in 0..512 {
					if let Some(tab) = self.child_table(i) {
						tab.print_to(d, addr << 9 | i);
					}
				}
			}
		}

		impl<L: Level> TablePrint for Table<L> {
			default fn print_to(&self, d: &mut DebugMap, addr: usize) {
				struct Mapping(VirtualAddress, PhysicalAddress, usize);

				impl Debug for Mapping {
					fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
						write!(f, "{:#0width$x} -> {:#0width$x}", self.0.0, self.0.0 + self.2, width = mem::size_of::<*mut u8>() * 2 + 2)
					}
				}

				let ranges = self.entries.iter().enumerate().filter_map(|(idx, entry)| {
					entry.pointed_frame().map(|frame| {
						let addr = (addr << 9 | idx) << 12;
						Mapping(VirtualAddress(addr), frame.start(), 4096)
					})
				});

				let last_item = ranges.reduce(|old_range, new_range| {
					if old_range.0.0 + old_range.2 == new_range.0.0 && old_range.1.0 + old_range.2 == new_range.1.0 {
						Mapping(old_range.0, old_range.1, old_range.2 + new_range.2)
					} else {
						d.entry(&old_range, &old_range.1.0);
						new_range
					}
				});

				if let Some(item) = last_item {
					d.entry(&item, &item.1.0);
				};
			}
		}

		write!(f, "PageTable ")?;
		let mut d = f.debug_map();
		self.0.table().print_to(&mut d, 0);
		d.finish()
	}
}

impl<'a, L: Level> Debug for TableTableDebugger<'a, L> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		<Table<L> as Debug>::fmt(&self.0, f)
	}
}

pub static CURRENT_PAGE_TABLE: Mutex<ActiveTable> = Mutex::new(unsafe { ActiveTable::new() });

pub struct InactiveTable {
	frame: Frame
}

impl InactiveTable {
	pub fn new() -> Result<Self, AllocError> {
		let f = phys::Global.allocate_one_aligned(mem::align_of::<Table<L4>>().ilog2() - 12)?;
		let mut table = Self {
			frame: f
		};

		CURRENT_PAGE_TABLE.lock().unwrap().modify_with(&mut table, |mapper| mapper.table_mut().clear());

		Ok(table)
	}
}

mod hooks {
	#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
	pub use x86::*;
	#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
	mod x86 {
		use core::arch::asm;
		use kernel_exports::memory::Page;

		pub fn unmap_page(page: Page) {
			unsafe { asm!("invlpg [{}]", in(reg) page.start().0) }
		}
	}
}
