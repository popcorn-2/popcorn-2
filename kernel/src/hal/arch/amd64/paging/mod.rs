use core::arch::asm;
use core::fmt::{Debug, Formatter};
use core::ops::{DerefMut, Range};
use kernel_api::allocator::{AllocError, highmem, DynPmm, Pmm};
use kernel_api::memory::{Frames, RawFrame, RawPage, VirtualAddress};
use kernel_api::sync::{Spinlock, SpinlockGuard};
#[cfg(debug_assertions)] use kernel_api::sync::Once;
use table::{Table, PDPT, PML4, PageIndices, Level, PD};
use crate::hal::paging2::{Flags, KTable, TTable};
use entry::Amd64Entry;
use kernel_api::mapping::Ty;
use crate::hal::paging::{Entry, MapPageError};
use crate::non_zero;

mod table;
mod entry;

pub(crate) unsafe fn construct_tables() -> (Amd64KTable, Amd64TTable) {
	#[cfg(debug_assertions)] {
		static CALLED: Once = Once::new();
		if CALLED.is_complete() { panic!("cannot call `construct_tables()` more than once") }
		CALLED.call_once(|| {});
	}

	let ttable_base: usize;
	unsafe {
		asm!(
			"mov {}, cr3",
		out(reg) ttable_base)
	};
	let ttable_base = ttable_base & 0xffff_ffff_ffff_f000;

	let ttable = unsafe {
		Amd64TTable::from_raw(RawFrame::new(ttable_base))
	};

	let ktable_base = ttable.pml4.pml4().entries[256].pointed_frame(false)
		.expect("Invalid TTable");

	let ktable = Amd64KTable {
		tables: KTablePtr(unsafe {
			Frames::from_raw(Range {
				start: ktable_base,
				end: ktable_base + 256usize,
			}, highmem()) // fixme: is highmem always correct
		}),
		allocator: highmem(),
	};

	(ktable, ttable)
}

#[derive(Debug)]
struct KTablePtr(Frames<true, Table<PDPT>>); // points to a [Table<PDPT>; 256]

#[repr(align(8))]
pub struct Amd64KTable {
	tables: KTablePtr, // points to a [Table<PDPT>; 256]
	allocator: DynPmm<'static, true>,
}

#[cfg(feature = "hal-next")]
impl Amd64KTable {
	pub unsafe fn from_raw(frame: RawFrame) -> Self {
		Amd64KTable {
			tables: KTablePtr(unsafe {
				Frames::from_raw(Range {
					start: frame,
					end: frame + 256usize,
				}, highmem()) // fixme: is highmem always correct
			}),
			allocator: highmem(),
		}
	}
}

impl KTablePtr {
	fn tables(&self) -> &[Table<PDPT>; 256] {
		self.0.get().try_into().expect("KTable not big enough")
	}

	fn tables_mut(&mut self) -> &mut [Table<PDPT>; 256] {
		self.0.get_mut().try_into().expect("KTable not big enough")
	}
}

impl Debug for Amd64KTable {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		Debug::fmt(&self.tables, f)
	}
}

#[repr(C)]
pub struct TTablePtr {
	pml4: Spinlock<Frames<true, Table<PML4>>>, // points to a Table<PML4>
}

impl TTablePtr {
	fn pml4(&self) -> impl DerefMut<Target = Table<PML4>> + '_ {
		let guard = self.pml4.lock();
		SpinlockGuard::map(guard, move |frames| &mut frames.get_mut()[0])
	}

	fn pml4_mut(&mut self) -> &mut Table<PML4> {
		&mut self.pml4.get_mut().get_mut()[0]
	}
}

trait TableUnmap {
	fn do_unmap(&mut self, page: RawPage, allocator: DynPmm<'static, true>) -> Result<(), ()>;
}

macro_rules! impl_table_unmap_parent {
    ($ty:ty) => {
	    impl TableUnmap for Table<$ty> {
			fn do_unmap(&mut self, page: RawPage, allocator: DynPmm<'static, true>) -> Result<(), ()> {
				// this weird setup is so we drop any references to the child table before deallocating it
				if let Some(child_frame) = {
					let child = self.child_table_mut(page).ok_or(())?;
					child.do_unmap(page, allocator)?;
					child.is_empty().then(|| RawFrame::new(VirtualAddress::from(child as *mut _).to_physical().addr))
				} {
					debug_assert!(self[page].is_present());
					self[page] = Amd64Entry::empty();
					unsafe { allocator.deallocate_raw(child_frame, non_zero!(1)) };
				}

				Ok(())
			}
		}
    };
}

impl_table_unmap_parent!(PML4);
impl_table_unmap_parent!(PDPT);
impl_table_unmap_parent!(PD);

impl<L: Level> TableUnmap for Table<L> {
	default fn do_unmap(&mut self, page: RawPage, _allocator: DynPmm<'static, true>) -> Result<(), ()> {
		assert_eq!(L::SHIFT, 12, "sanity check on specialisation");

		let entry = &mut self[page];
		match (entry.is_used(), entry.is_present()) {
			(true, false) => {
				todo!("remove page from swap")
			},
			(true, true) => {
				*entry = Amd64Entry::empty();
				unsafe { asm!("invlpg [{}]", in(reg) page.addr); }
				Ok(())
			},
			(false, _) => Err(())
		}
	}
}

pub struct Amd64TTable {
	pub pml4: TTablePtr,
	allocator: DynPmm<'static, true>,
	// todo: pcid: u16,
}

impl Amd64TTable {
	pub unsafe fn from_raw(frame: RawFrame) -> Self {
		let pml4 = unsafe {
			Frames::from_raw(
				Range {
					start: frame,
					end: frame + 1usize,
				},
				highmem(), // fixme: is highmem always correct
			)
		};

		Self {
			pml4: TTablePtr {
				pml4: Spinlock::new(pml4),
			},
			allocator: highmem(),
		}
	}
}

impl Debug for Amd64TTable {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		Debug::fmt(&*self.pml4.pml4(), f)
	}
}

impl Amd64TTable {
	fn do_map(pml4: &mut Table<PML4>, page: RawPage, frame: RawFrame, ty: Ty, flags: Flags, allocator: DynPmm<'static, true>) -> Result<(), MapPageError> {
		assert!(!page.is_higher_half(), "TTable only handles lower half addresses");
		
		trace!("=== map va {:#018x} -> pa {:#018x} : ty={ty:?}", page, frame);

		let pdpt = pml4.child_table_or_new(page, allocator)?;
		let pd = pdpt.child_table_or_new(page, allocator)?;
		let pt = pd.child_table_or_new(page, allocator)?;
		pt[page].point_to_frame(frame, ty, flags).map_err(|e| MapPageError::AlreadyMapped(e))
	}

	fn do_unmap(pml4: &mut Table<PML4>, page: RawPage, allocator: DynPmm<'static, true>) -> Result<(), ()> {
		assert!(!page.is_higher_half(), "TTable only handles lower half addresses");

		trace!("=== unmap va {:#018x}", page);

		let pdpt = pml4.child_table_mut(page).ok_or(())?;
		pdpt.do_unmap(page, allocator)
	}
}

impl KTable for Amd64TTable {
	fn translate_page(&self, page: RawPage, debug: bool) -> Option<RawFrame> {
		assert!(!page.is_higher_half(), "TTable only handles lower half addresses");

		let pml4 = self.pml4.pml4();
		let pdpt = pml4.child_table(page)?;
		let pd = pdpt.child_table(page)?;
		let pt = pd.child_table(page)?;
		pt[page].pointed_frame(debug)
	}

	fn map_page(&mut self, page: RawPage, frame: RawFrame, ty: Ty, flags: Flags) -> Result<(), MapPageError> {
		Self::do_map(
			self.pml4.pml4_mut(),
			page,
			frame,
			ty,
			flags,
			self.allocator,
		)
	}

	fn unmap_page(&mut self, page: RawPage) -> Result<(), ()> {
		Self::do_unmap(self.pml4.pml4_mut(), page, self.allocator)
	}
}

impl KTable for Amd64KTable {
	fn translate_page(&self, page: RawPage, debug: bool) -> Option<RawFrame> {
		assert!(page.is_higher_half(), "KTable only handles lower half addresses");

		let pdpt = &self.tables.tables()[page.index::<PML4>() - 256];
		let pd = pdpt.child_table(page)?;
		let pt = pd.child_table(page)?;
		pt[page].pointed_frame(debug)
	}

	fn map_page(&mut self, page: RawPage, frame: RawFrame, ty: Ty, flags: Flags) -> Result<(), MapPageError> {
		assert!(page.is_higher_half(), "KTable only handles upper half addresses");

		trace!("=== map va {:#018x} -> pa {:#018x} : ty={ty:?}", page, frame);

		let pdpt = &mut self.tables.tables_mut()[page.index::<PML4>() - 256];
		let pd = pdpt.child_table_or_new(page, self.allocator)?;
		let pt = pd.child_table_or_new(page, self.allocator)?;
		pt[page].point_to_frame(frame, ty, flags).map_err(|e| MapPageError::AlreadyMapped(e))
	}

	fn unmap_page(&mut self, page: RawPage) -> Result<(), ()> {
		assert!(page.is_higher_half(), "KTable only handles upper half addresses");

		trace!("=== unmap va {:#018x}", page);

		let pdpt = &mut self.tables.tables_mut()[page.index::<PML4>() - 256];
		pdpt.do_unmap(page, self.allocator)
	}
}

impl TTable for Amd64TTable {
	unsafe fn load(&self) -> usize {
		let frame = unsafe { self.pml4.pml4.data_ptr().base_raw() };
		unsafe { Self::load_raw(frame.addr) }
	}
	
	unsafe fn load_raw(addr: usize) -> usize {
		let old_addr;
		unsafe { asm!("mov cr3, {}", inout(reg) addr => old_addr); }
		old_addr
	}

	fn new(ktable: &Amd64KTable, allocator: DynPmm<'static, true>) -> Result<Self, AllocError> {
		let pml4_frame = Table::<PML4>::empty_with(allocator)?;
		let pml4 = pml4_frame.to_virtual().as_ptr().cast::<Table<PML4>>();
		assert!(!pml4.is_null() && pml4.is_aligned());
		let pml4 = unsafe { &mut *pml4 };

		for (i, entry) in pml4.entries[256..].iter_mut().enumerate() {
			let ktable_frame = ktable.tables.0.base() + i;
			entry.point_to_frame(ktable_frame, Ty(0), Flags::WRITE | Flags::EXEC)
					.expect("Empty table should have no mappings");
		}

		Ok(Self {
			pml4: TTablePtr {
				pml4: Spinlock::new(
					unsafe {
						Frames::from_raw(Range {
							start: pml4_frame,
							end: pml4_frame + 1usize,
						}, allocator)
					}
				),
			},
			allocator
		})
	}

	fn map_page(&self, page: RawPage, frame: RawFrame, ty: Ty, flags: Flags) -> Result<(), MapPageError> {
		Self::do_map(
			&mut *self.pml4.pml4(),
			page,
			frame,
			ty,
			flags,
			self.allocator,
		)
	}

	fn unmap_page(&self, page: RawPage) -> Result<(), ()> {
		Self::do_unmap(
			&mut *self.pml4.pml4(),
			page,
			self.allocator,
		)
	}
}
