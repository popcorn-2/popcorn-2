use core::fmt::Debug;
use kernel_api::bridge::paging::MapPageError;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};
use crate::{Hal, HalTy};

pub type KTableTy = <HalTy as crate::Hal>::KTableTy;
pub type TTableTy = <HalTy as crate::Hal>::TTableTy;

pub unsafe fn construct_tables() -> (KTableTy, TTableTy) {
	<HalTy as crate::Hal>::construct_tables()
}

pub trait KTable: Debug {
	fn translate_page(&self, page: Page) -> Option<Frame>;

	fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
		let aligned = addr.align_down();
		let diff = addr - aligned;
		let physical = self.translate_page(Page::new(aligned))?;
		Some(physical.start() + diff)
	}

	fn map_page(&mut self, page: Page, frame: Frame) -> Result<(), MapPageError>;
}

pub trait TTable: KTable {
	/// # Safety
	///
	/// Page table must be alive until unloaded
	///
	/// # To Do
	///
	/// Figure out a better signature involving `Arc` or something
	unsafe fn load(&self);
}

trait HackyExportTrick: KTable {
	fn translate_page(&self, page: Page) -> Option<Frame>;
	fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress>;
	fn map_page(&mut self, page: Page, frame: Frame) -> Result<(), MapPageError>;
}

impl HackyExportTrick for <HalTy as Hal>::KTableTy {
	#[export_name = "__popcorn_paging_ktable_translate_page"]
	fn translate_page(&self, page: Page) -> Option<Frame> {
		<Self as KTable>::translate_page(self, page)
	}

	#[export_name = "__popcorn_paging_ktable_translate_address"]
	fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
		<Self as KTable>::translate_address(self, addr)
	}

	#[export_name = "__popcorn_paging_ktable_map_page"]
	fn map_page(&mut self, page: Page, frame: Frame) -> Result<(), MapPageError> {
		<Self as KTable>::map_page(self, page, frame)
	}
}
