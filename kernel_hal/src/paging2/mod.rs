use core::fmt::Debug;
use kernel_api::bridge::paging::MapPageError;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress};

pub type KTableTy = <crate::HalTy as crate::Hal>::KTableTy;
pub type TTableTy = <crate::HalTy as crate::Hal>::TTableTy;

pub unsafe fn construct_tables() -> (KTableTy, TTableTy) {
	<crate::HalTy as crate::Hal>::construct_tables()
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
