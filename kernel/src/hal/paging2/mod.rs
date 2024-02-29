use core::fmt::Debug;
use kernel_api::bridge::paging::MapPageError;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress, AllocError};
use crate::{Hal, HalTy};
use kernel_api::memory::allocator::{BackingAllocator};

pub type KTableTy = <HalTy as crate::Hal>::KTableTy;
pub type TTableTy = <HalTy as crate::Hal>::TTableTy;

pub unsafe fn construct_tables() -> (KTableTy, TTableTy) {
	<HalTy as crate::Hal>::construct_tables()
}

pub trait KTable: Debug + Sized {
	fn translate_page(&self, page: Page) -> Option<Frame>;

	fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
		let aligned = addr.align_down();
		let diff = addr - aligned;
		let physical = self.translate_page(Page::new(aligned))?;
		Some(physical.start() + diff)
	}

	fn map_page(&mut self, page: Page, frame: Frame) -> Result<(), MapPageError>;
	fn unmap_page(&mut self, page: Page) -> Result<(), ()>;
}

pub trait TTable: KTable + Sized {
	type KTableTy: KTable;

	/// # Safety
	///
	/// Page table must be alive until unloaded
	///
	/// # To Do
	///
	/// Figure out a better signature involving `Arc` or something
	unsafe fn load(&self);

	fn new(ktable: &Self::KTableTy, allocator: &'static dyn BackingAllocator) -> Result<Self, AllocError>;
}

#[export_name = "__popcorn_paging_ktable_translate_page"]
fn translate_page(this: &<HalTy as Hal>::KTableTy, page: Page) -> Option<Frame> {
	<<HalTy as Hal>::KTableTy as KTable>::translate_page(this, page)
}

#[export_name = "__popcorn_paging_ktable_translate_address"]
fn translate_address(this: &<HalTy as Hal>::KTableTy, addr: VirtualAddress) -> Option<PhysicalAddress> {
	<<HalTy as Hal>::KTableTy as KTable>::translate_address(this, addr)
}

#[export_name = "__popcorn_paging_ktable_map_page"]
fn map_page(this: &mut <HalTy as Hal>::KTableTy, page: Page, frame: Frame) -> Result<(), MapPageError> {
	<<HalTy as Hal>::KTableTy as KTable>::map_page(this, page, frame)
}

#[export_name = "__popcorn_paging_ktable_unmap_page"]
fn unmap_page(this: &mut <HalTy as Hal>::KTableTy, page: Page) -> Result<(), ()> {
	<<HalTy as Hal>::KTableTy as KTable>::unmap_page(this, page)
}
