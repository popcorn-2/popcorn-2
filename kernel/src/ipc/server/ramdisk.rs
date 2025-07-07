use alloc::sync::Arc;
use crate::prelude::*;
use core::sync::atomic::{AtomicUsize, Ordering};
use kernel_api::sync::OnceLock;
use crate::ipc::{Error, protocol};
use crate::ipc::ctor::{CtorContext, ProtocolVisitor};
use crate::ipc::handle::Handle;
use crate::ipc::protocol::DispatchTable;
use crate::ipc::server::{ReturnHandle, Server};

#[derive(Debug)]
pub struct RamdiskServer {
	position: AtomicUsize,
	data: Box<[u8]>,
}

impl RamdiskServer {
	pub fn new(data: Box<[u8]>) -> Self {
		Self {
			position: AtomicUsize::new(0),
			data
		}
	}
}

impl Server for RamdiskServer {
	type CtorContext = CtorCtx;

	fn ctor(&self, _endpoint: &str, _ctx: Self::CtorContext) -> Result<ReturnHandle, Error> {
		Err(Error::UnsupportedProtocol)
	}

	fn destroy(&self, _handle: isize) -> Result<(), Error> { Ok(()) }

	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH_TABLE: OnceLock<DispatchTable> = OnceLock::new();
		DISPATCH_TABLE.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as protocol::generated::CoreIoRead>::__vtable())
		)
	}
}

impl protocol::generated::CoreIoRead for RamdiskServer {
	async fn new_from(&self, _: &str, _: Arc<Handle>) -> Result<ReturnHandle, Error> { Err(Error::UnsupportedProtocol) }

	async fn read(&self, _handle: isize, output_size: usize) -> Result<Box<[u8]>, Error> {
		let position = self.position.load(Ordering::Relaxed);
		if position >= self.data.len() { return Ok(Box::<[_]>::from([])); }
		
		if position.saturating_add(output_size) > self.data.len() {
			self.position.store(self.data.len(), Ordering::Relaxed);
			info!("read ramdisk from {position} to end ({})", self.data.len());
			Ok(Box::<[_]>::from(&self.data[position..]))
		} else {
			self.position.fetch_add(output_size, Ordering::Relaxed);
			info!("read ramdisk from {position} to {}", position+output_size);
			Ok(Box::<[_]>::from(&self.data[position..(position+output_size)]))
		}
	}
}

#[derive(Default)]
pub struct CtorCtx;

impl CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> { const { &ProtocolVisitor::new() } }
}
