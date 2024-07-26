#[allow(unused_imports)] use crate::prelude::*;

use core::fmt::Debug;
use kernel_api::bridge::paging::MapPageError;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress, AllocError};
use super::KTableTy;
use kernel_api::memory::allocator::{PhysicalAllocator};

pub trait KTable: Debug + Sized {
	fn translate_page(&self, page: Page) -> Option<Frame>;

	fn translate_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress> {
		let aligned = addr.align_down();
		let diff = addr - aligned;
		let physical = self.translate_page(Page::new(aligned))?;
		Some(physical.start() + diff)
	}

	fn map_page(&mut self, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError>;
	fn unmap_page(&mut self, page: Page) -> Result<(), ()>;
}

pub trait TTable: KTable + Sized {
	/// # Safety
	///
	/// Page table must be alive until unloaded
	///
	/// # To Do
	///
	/// Figure out a better signature involving `Arc` or something
	unsafe fn load(&self);

	fn new(ktable: &KTableTy, allocator: &'static dyn PhysicalAllocator) -> Result<Self, AllocError>;
}

#[export_name = "__popcorn_paging_ktable_translate_page"]
fn translate_page(this: &KTableTy, page: Page) -> Option<Frame> {
	<KTableTy as KTable>::translate_page(this, page)
}

#[export_name = "__popcorn_paging_ktable_translate_address"]
fn translate_address(this: &KTableTy, addr: VirtualAddress) -> Option<PhysicalAddress> {
	<KTableTy as KTable>::translate_address(this, addr)
}

#[export_name = "__popcorn_paging_ktable_map_page"]
fn map_page(this: &mut KTableTy, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError> {
	<KTableTy as KTable>::map_page(this, page, frame, reason)
}

#[export_name = "__popcorn_paging_ktable_unmap_page"]
fn unmap_page(this: &mut KTableTy, page: Page) -> Result<(), ()> {
	<KTableTy as KTable>::unmap_page(this, page)
}
