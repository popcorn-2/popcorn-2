use core::fmt::Debug;
use bitflags::bitflags;
use kernel_api::allocator::{AllocError, DynPmm};
use kernel_api::mapping::Ty;
use kernel_api::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};
use crate::hal::paging::MapPageError;
use crate::memory::paging::ktable;
use super::{KTableTy, TTableTy};

bitflags! {
	#[derive(Copy, Clone, Debug)]
	#[repr(transparent)]
	pub struct Flags: u8 {
		//const READ = 1 << 0;
		const WRITE = 1 << 1;
		const EXEC = 1 << 2;
		const USER = 1 << 3;
		const WRITE_COMBINE = 1 << 4;
		const UNCACHED = 1 << 5;
	}
}

pub trait KTable: Debug + Sized {
	fn translate_page(&self, page: RawPage, debug: bool) -> Option<RawFrame>;

	fn translate_address(&self, addr: VirtualAddress, debug: bool) -> Option<PhysicalAddress> {
		let aligned = addr.align_down_to_page();
		let diff = addr - *aligned;
		let physical = self.translate_page(aligned, debug)?;
		Some(*physical + diff)
	}

	fn map_page(&mut self, page: RawPage, frame: RawFrame, ty: Ty, flags: Flags) -> Result<(), MapPageError>;
	fn unmap_page(&mut self, page: RawPage) -> Result<(), ()>;
}

pub trait TTable: KTable + Sized {
	/// # Safety
	///
	/// - Page table must be alive until unloaded
	/// - All types held across the call to `load` must be [`Send`] and [`Sync`]
	unsafe fn load(&self) -> usize;
	
	/// # Safety
	/// 
	/// See safety requirements of [`TTable::load()`]
	unsafe fn load_raw(ptr: usize) -> usize;

	fn new(ktable: &KTableTy, allocator: DynPmm<'static, true>) -> Result<Self, AllocError>;

	fn map_page(&self, page: RawPage, frame: RawFrame, ty: Ty, flags: Flags) -> Result<(), MapPageError>;
	fn unmap_page(&self, page: RawPage) -> Result<(), ()>;
}

#[unsafe(no_mangle)]
fn __popcorn_kpt_map_contiguous(base_page: RawPage, base_frame: RawFrame, count: usize, ty: Ty, flags: Flags) -> Result<(), MapPageError> {
	let mut ktable = ktable();

	let mut remaining = count;
	while remaining > 0 {
		match ktable.map_page(
			base_page + (count - remaining),
			base_frame + (count - remaining),
			ty,
			flags,
		) {
			Ok(_) => {},
			Err(e) => {
				for page in base_page .. (base_page + (count - remaining)) {
					let res = ktable.unmap_page(page);
					debug_assert!(res.is_ok(), "just mapped this page");
				}
				return Err(e);
			}
		}
		remaining -= 1;
	}

	Ok(())
}

#[unsafe(no_mangle)]
fn __popcorn_kpt_unmap(page: RawPage) -> Result<(), ()> { ktable().unmap_page(page) }

#[unsafe(no_mangle)]
fn __popcorn_kpt_translate_page(page: RawPage) -> Option<RawFrame> { ktable().translate_page(page, false) }

#[unsafe(no_mangle)]
fn __popcorn_kpt_translate_addr(addr: VirtualAddress) -> Option<PhysicalAddress> { ktable().translate_address(addr, false) }

#[unsafe(no_mangle)]
fn __popcorn_upt_unmap(this: &TTableTy, page: RawPage) -> Result<(), ()> { this.unmap_page(page) }
