use alloc::rc::Rc;
use log::info;
use paging_macros::l4_table_ty;

extern "Rust" {
	fn __popcorn_paging_page_table_new() -> l4_table_ty!();
}

#[derive(Debug, Copy, Clone)]
pub struct Page(pub u64);
#[derive(Debug, Copy, Clone)]
pub struct Frame(pub u64);

pub struct PageTable;

impl PageTable {
	pub fn new() -> Result<PageTable, ()> {
		Ok(PageTable)
	}

	pub fn map_page(&mut self, page: Page, frame: Frame) -> Result<(),()> {
		info!("map {page:x?} to {frame:x?}");
		Ok(())
	}

	pub fn unmap_page(&mut self, page: Page) -> Result<Frame,()> {
		todo!()
	}
}
/*
trait ArchPageTable {
	fn new() -> Self;
}

mod amd64 {
	use paging_macros::page_table;
	use crate::paging::ArchPageTable;

	#[page_table(4)]
	pub struct Level4PageTable {
		entries: [u64; 512]
	}

	impl ArchPageTable for Level4PageTable {
		fn new() -> Self {
			todo!()
		}
	}
}
*/