#[allow(unused_imports)] use crate::prelude::*;

use core::fmt::Debug;
use kernel_api::bridge::paging::MapPageError;
use kernel_api::memory::{Frame, Page, PhysicalAddress, VirtualAddress, AllocError};
use super::{KTableTy, TTableTy};
use kernel_api::memory::allocator::{PhysicalAllocator};
use crate::memory::r#virtual::AddressSpaceInner;

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

	fn map_page(&self, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError>;
	fn unmap_page(&self, page: Page) -> Result<(), ()>;
}

#[no_mangle]
fn __popcorn_paging_ktable_translate_page(this: &KTableTy, page: Page) -> Option<Frame> {
	<KTableTy as KTable>::translate_page(this, page)
}

#[no_mangle]
fn __popcorn_paging_ktable_translate_address(this: &KTableTy, addr: VirtualAddress) -> Option<PhysicalAddress> {
	<KTableTy as KTable>::translate_address(this, addr)
}

#[no_mangle]
fn __popcorn_paging_ktable_map_page(this: &mut KTableTy, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError> {
	<KTableTy as KTable>::map_page(this, page, frame, reason)
}

#[no_mangle]
fn __popcorn_paging_ktable_unmap_page(this: &mut KTableTy, page: Page) -> Result<(), ()> {
	<KTableTy as KTable>::unmap_page(this, page)
}

#[no_mangle]
fn __popcorn_paging_ttable_translate_page(this: &AddressSpaceInner, page: Page) -> Option<Frame> {
	<TTableTy as KTable>::translate_page(this.ttable(), page)
}

#[no_mangle]
fn __popcorn_paging_ttable_translate_address(this: &AddressSpaceInner, addr: VirtualAddress) -> Option<PhysicalAddress> {
	<TTableTy as KTable>::translate_address(this.ttable(), addr)
}

#[no_mangle]
fn __popcorn_paging_ttable_map_page(this: &AddressSpaceInner, page: Page, frame: Frame, reason: u16) -> Result<(), MapPageError> {
	<TTableTy as TTable>::map_page(this.ttable(), page, frame, reason)
}

#[no_mangle]
fn __popcorn_paging_ttable_unmap_page(this: &AddressSpaceInner, page: Page) -> Result<(), ()> {
	<TTableTy as TTable>::unmap_page(this.ttable(), page)
}
