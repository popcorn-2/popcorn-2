use alloc::sync::Arc;
use core::num::NonZero;
use slab::Slab;
use kernel_api::allocator::highmem;
use kernel_api::memory::{Frames, PAGE_SIZE};
use kernel_api::sync::{OnceLock, Spinlock};
use crate::ipc::ctor::{CtorContext, ProtocolVisitor};
use crate::ipc::{Error, protocol};
use kernel_api::syscall::handle::Handle;
use crate::ipc::protocol::DispatchTable;
use crate::ipc::server::{ReturnHandle, Server};
use crate::non_zero;

#[derive(Debug)]
pub struct MemServer {
	allocations: Spinlock<Slab<Frames<true>>>,
}

impl MemServer {
	pub fn new() -> Self { Self { allocations: Spinlock::new(Slab::new()) } }
}

impl Server for MemServer {
	type CtorContext = CtorCtx;

	async fn ctor(&self, endpoint: &str, _ctx: CtorCtx) -> Result<ReturnHandle, Error> {
		let size = endpoint.parse::<NonZero<usize>>()
				.map_err(|_| Error::InvalidArg)?;
		if size.get() % PAGE_SIZE != 0 { return Err(Error::InvalidArg); }
		let size = size.div_ceil(non_zero!(PAGE_SIZE));

		let frames = highmem().allocate(size)?;
		let key = self.allocations.lock().insert(frames);
		Ok(ReturnHandle::NewDefault(key as isize))
	}

	async fn destroy(&self, handle: isize) -> Result<(), Error> {
		self.allocations.lock().try_remove(handle as usize)
				.map(|_| ())
				.ok_or(Error::InvalidHandle)
	}

	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH_TABLE: OnceLock<DispatchTable> = OnceLock::new();
		DISPATCH_TABLE.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as protocol::generated::core::mem::Pager>::__vtable())
		)
	}
}

impl protocol::generated::core::mem::Pager for MemServer {
	async fn new_from(&self, _: &str, _: Arc<Handle>) -> Result<ReturnHandle, Error> { Err(Error::UnsupportedProtocol) }

	async fn get_pages(&self, handle: isize, offset: usize) -> Result<*const u8, Error> {
		if offset != 0 { return Err(Error::FutureCompat); }

		let guard = self.allocations.lock();
		let frames = guard.get(handle as usize).ok_or(Error::InvalidHandle)?;
		Ok(core::ptr::without_provenance(frames.base().addr))
	}
}


#[derive(Default)]
pub struct CtorCtx;

impl CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> {
		static VISITORS: OnceLock<ProtocolVisitor<CtorCtx>> = OnceLock::new();

		VISITORS.get_or_init(||
				ProtocolVisitor::new()
						.add_visitor::<dyn protocol::generated::core::mem::Pager>(|_, _| Ok(()))
		)
	}
}
