use alloc::boxed::Box;
use core::arch::asm;
use core::error::Error;
use core::{fmt, ptr};
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::{Index as _, IndexMut as _};
use bitflags::bitflags;
use log::trace;
use uefi::boot;
use uefi::boot::{AllocateType, MemoryType};
use kernel_api::mapping::Ty;
use kernel_api::memory::{RawFrame, RawPage, VirtualAddress};
use utils::handoff;

pub fn final_init() {
	// SAFETY: only enable hardware features
	unsafe {
		// Enable write-protect bit
		asm!(
			"mov {0:r}, cr0",
			"or {0:r}, 0x10000",
			"mov cr0, {0:r}",
			out(reg) _
		);
		// Enable NX enable and syscall/sysret bits
		asm!(
			"rdmsr",
			"or eax, 0x801",
			"wrmsr",
			in("ecx") 0xC000_0080u32,
			out("eax") _,
			out("edx") _
		);
	}
}

pub fn handover(entry: VirtualAddress, stack_ptr: VirtualAddress, handoff: *const handoff::Data) -> ! {
	let stack_ptr = stack_ptr.addr & (-16isize).cast_unsigned();
	// SAFETY: never return to rust code from this again
	unsafe {
		asm!(
			"wrmsr",

			"mov rsp, {stack}",

			"cld", // clear direction flag

			"push 0",
			"xor ebp, ebp",

			"jmp {entry}",

			stack = in(reg) stack_ptr,
			entry = in(reg) entry.addr,
			in("rdi") handoff,
			in("rax") 0xead10ca1u32,
			in("rdx") 0xdu32, // edx:eax = 0xdead10ca1 ('dead local')
			in("rcx") 0xc0000101u32, // ecx = GSBase MSR
			options(noreturn))
	}
}

#[expect(clippy::unusual_byte_groupings, reason = "clearer for page indices")]
trait PageExt {
	const L4_SHIFT: usize = 12 + 9*3;
	const L3_SHIFT: usize = 12 + 9*2;
	const L2_SHIFT: usize = 12 + 9;
	const L1_SHIFT: usize = 12;
	const L4_MASK:  usize = 0o777_000_000_000_0000;
	const L3_MASK:  usize =     0o777_000_000_0000;
	const L2_MASK:  usize =         0o777_000_0000;
	const L1_MASK:  usize =             0o777_0000;
	
	fn l4_index(self) -> usize;
	fn l3_index(self) -> usize;
	fn l2_index(self) -> usize;
	fn l1_index(self) -> usize;
}

impl PageExt for RawPage {
	fn l4_index(self) -> usize { (self.addr & Self::L4_MASK) >> Self::L4_SHIFT }
	fn l3_index(self) -> usize { (self.addr & Self::L3_MASK) >> Self::L3_SHIFT }
	fn l2_index(self) -> usize { (self.addr & Self::L2_MASK) >> Self::L2_SHIFT }
	fn l1_index(self) -> usize { (self.addr & Self::L1_MASK) >> Self::L1_SHIFT }
}

#[derive(Debug)]
#[repr(C, align(4096))]
struct Table<Level: TableLevel>([TableEntry; 512], PhantomData<Level>);

impl<Level: TableLevel> Table<Level> {
	const fn new() -> Self {
		Self([const { TableEntry::new() }; 512], PhantomData)
	}
}

impl<Level: ParentTableLevel> Table<Level> {
	pub fn get_child_table(&self, index: usize) -> Option<&'static Table<Level::Child>> {
		// SAFETY: All memory in preboot environment is identity mapped, and page tables
		//  only point to valid memory
		self.0[index].pointed_frame()
			.map(|frame| unsafe { &*ptr::with_exposed_provenance(frame.addr) })
	}

	/// # Errors
	///
	/// Returns the firmware error if memory allocation failed.
	fn try_get_or_create_child_table(&mut self, index: usize) -> uefi::Result<&'static mut Table<Level::Child>> {
		let entry = &mut self.0[index];
		entry.pointed_frame()
			.map_or_else(|| {
				debug_assert!(Level::Child::VALUE != 3 || index < 256, "L3 table did not exist already - if this is not in lower half, this is a bug");
				let mut table_ptr = boot::allocate_pages(
					AllocateType::AnyPages,
					MemoryType::LOADER_DATA,
					1,
				)?.cast::<MaybeUninit<Table<_>>>();
				// SAFETY: firmware returns unaliased writable memory
				let table = unsafe { table_ptr.as_mut() };
				let table = table.write(Table::new());
				#[expect(clippy::missing_panics_doc, reason = "infallible")]
				entry.set_pointed_frame(RawFrame::new(table_ptr.expose_provenance().get()), TableEntryFlags::PERMISSIVE).expect("just checked there was no frame mapped");
				Ok(table)
			}, |frame| {
				// SAFETY: All memory in preboot environment is identity mapped, and page tables
				//  only point to valid memory
				Ok(unsafe { &mut *ptr::with_exposed_provenance_mut(frame.addr) })
			})
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

#[repr(transparent)]
struct TableEntry(usize);

impl fmt::Debug for TableEntry {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let frame = self.pointed_frame();
		let flags = self.flags();
		f.debug_struct("TableEntry")
			.field("frame", &frame)
			.field("flags", &flags)
			.finish()
	}
}

bitflags! {
	#[derive(Copy, Clone, Debug)]
	pub struct TableEntryFlags: usize {
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

		const PERMISSIVE =      Self::WRITABLE.bits() | Self::USER_ACCESSIBLE.bits();
		const MMIO =            Self::WRITE_THROUGH.bits() | Self::NO_CACHE.bits();

		const _ = !0; // prevent operators from truncating value
	}
}

impl TableEntryFlags {
	pub fn from_ty(ty: Ty) -> Self {
		let reason = usize::from(ty.0);
		let low = (reason & 7) << 9;
		let high = (reason & 0x3ff8) << (52 - 3);
		Self::from_bits_retain(low | high)
	}

	pub const fn to_ty(self) -> Ty {
		let low = (self.bits() >> 9) & 7;
		let high = (self.bits() >> (52 - 3)) & 0x3ff8;
		Ty((low | high) as u8)
	}
}

impl TableEntry {
	const fn new() -> Self { Self(TableEntryFlags::WRITABLE.bits()) }

	const fn flags(&self) -> TableEntryFlags {
		TableEntryFlags::from_bits_truncate(self.0)
	}

	const fn pointed_frame(&self) -> Option<RawFrame> {
		if self.flags().contains(TableEntryFlags::PRESENT) {
			Some(RawFrame::new(self.0 & 0x000f_ffff_ffff_f000))
		} else { None }
	}

	fn set_pointed_frame_unchecked(&mut self, frame: RawFrame, flags: TableEntryFlags) {
		self.0 = 0;
		self.0 |= frame.addr & 0x000f_ffff_ffff_f000;
		self.0 |= (flags | TableEntryFlags::PRESENT).bits();
	}

	/// # Errors
	///
	/// Returns an error if the page is already mapped.
	fn set_pointed_frame(&mut self, frame: RawFrame, flags: TableEntryFlags) -> Result<(), AlreadyMappedError> {
		if self.pointed_frame().is_some() {
			Err(AlreadyMappedError(self.flags().to_ty()))
		} else {
			self.set_pointed_frame_unchecked(frame, flags);
			Ok(())
		}
	}
}

pub struct PageTable(&'static mut Table<Level4>);

#[derive(Debug)]
pub struct AlreadyMappedError(Ty);

impl fmt::Display for AlreadyMappedError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "address is already mapped for type {:?}", self.0)
	}
}

impl Error for AlreadyMappedError {}

impl PageTable {
	/// # Errors
	///
	/// Returns an error on failure to allocate memory for the page table.
	pub fn new() -> Result<Self, Box<dyn Error>> {
		const { assert!(align_of::<Table<Level4>>() <= boot::PAGE_SIZE, "Table alignment must fit in UEFI provided alignment"); }

		let mut l4 = boot::allocate_pages(
			AllocateType::AnyPages,
			MemoryType::LOADER_DATA,
			1,
		)?.cast::<MaybeUninit<Table<Level4>>>();
		// SAFETY: firmware returns unaliased writable memory
		let l4 = unsafe { l4.as_mut() }.write(Table::new());

		let mut l3_kernel_tables = boot::allocate_pages(
			AllocateType::AnyPages,
			MemoryType::LOADER_DATA,
			256,
		)?.cast::<[MaybeUninit<Table<Level3>>; 256]>();
		// SAFETY: firmware returns unaliased writable memory
		let l3_kernel_tables = unsafe { l3_kernel_tables.as_mut() };

		for (i, l3) in l3_kernel_tables.iter_mut().enumerate() {
			let l3 = l3.write(Table::new());
			#[expect(clippy::missing_panics_doc, reason = "infallible")]
			l4.0[i + 256].set_pointed_frame(
				RawFrame::new(
					(&raw const *l3).expose_provenance()
				),
				TableEntryFlags::PERMISSIVE,
			).expect("just created an empty page table");
		}

		Ok(Self(l4))
	}

	/// # Errors
	///
	/// Returns an error if memory allocation failed, or a page is already mapped.
	pub fn try_map_range_with(&mut self, page_start: RawPage, frame_start: RawFrame, count: usize, flags: TableEntryFlags, ty: Ty) -> Result<(), Box<dyn Error>> {
		for i in 0..count {
			let page = page_start + i;
			let frame = frame_start + i;

			let entry = self.0.try_get_or_create_child_table(page.l4_index())?
				.try_get_or_create_child_table(page.l3_index())?
				.try_get_or_create_child_table(page.l2_index())?
				.0.index_mut(page.l1_index());

			let flags = flags | TableEntryFlags::from_ty(ty);
			entry.set_pointed_frame(frame, flags)?;
			trace!(target: "vmsan", "=== map va {page:#018x} -> pa {frame:#018x} : ty={ty:?}");
		}
		Ok(())
	}

	pub fn translate_page(&self, page: RawPage) -> Option<RawFrame> {
		let entry = self.0.get_child_table(page.l4_index())?
			.get_child_table(page.l3_index())?
			.get_child_table(page.l2_index())?
			.0.index(page.l1_index());
		entry.pointed_frame()
	}

	/// # Safety
	///
	/// Must not invalidate or modify any outstanding references.
	pub unsafe fn switch(&self) {
		let addr = ptr::from_ref(self.0);
		// SAFETY: upheld by caller
		unsafe { asm!("mov cr3, {}", in(reg) addr, options(nostack, preserves_flags)); }
	}

	pub fn handoff_u_table(&self) -> RawFrame {
		RawFrame::new(
			(&raw const *self.0).addr()
		)
	}

	pub fn handoff_s_table(&self) -> RawFrame {
		self.0.0[256].pointed_frame()
			.expect("STable must exist")
	}
}
