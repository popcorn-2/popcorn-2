use crate::prelude::*;
use alloc::string::String;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering};
use kernel_api::sync::OnceLock;
use crate::hal::FormatWriter;
use crate::ipc::ctor::{CtorContext, ProtocolVisitor};
use crate::ipc::{Error, protocol};
use crate::ipc::dispatch::Return;
use crate::ipc::handle::Handle;
use crate::ipc::protocol::DispatchTable;
use crate::ipc::server::{ReturnHandle, Server};

/// IO server for the kernel debug console
///
/// Probably temporary jank until we have userspace drivers working
///
/// Provides a single object at `/` implementing `core.io.Read` and `core.io.Write`
///
/// The object is internally locked, preventing multiple processes from opening it simultaneously
#[derive(Debug)]
pub struct ConsoleServer {
	lock: AtomicBool,
}

impl ConsoleServer {
	pub fn new() -> Self { Self { lock: AtomicBool::new(false) } }
}

impl Server for ConsoleServer {
	type CtorContext = CtorCtx;

	fn ctor(&self, endpoint: &str, _ctx: CtorCtx) -> Result<ReturnHandle, Error> {
		if endpoint != "" { return Err(Error::EndpointNotFound); }
		
		let res = self.lock.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed);
		if res.is_ok() { Ok(ReturnHandle::NewDefault(1)) }
		else { Err(Error::NameInUse) }
	}

	fn destroy(&self, handle: isize) -> Result<(), Error> {
		if handle != 1 { return Err(Error::InvalidHandle); }
		self.lock.store(false, Ordering::Relaxed);
		Ok(())
	}
	
	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH_TABLE: OnceLock<DispatchTable> = OnceLock::new();
		DISPATCH_TABLE.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as protocol::generated::CoreIoWrite>::__vtable())
				.add_vtable(<Self as protocol::generated::CoreIoRead>::__vtable())
		)
	}
}

impl protocol::generated::CoreIoWrite for ConsoleServer {
	async fn new_from(&self, _: &str, _: Arc<Handle>) -> Result<ReturnHandle, Error> { Err(Error::UnsupportedProtocol) }

	async fn write(&self, _handle: isize, buf: &[u8]) -> Result<usize, Error> {
		let s = String::from_utf8_lossy(buf);
		sprint!("[C] {s}");
		Ok(buf.len())
	}
}

impl protocol::generated::CoreIoRead for ConsoleServer {
	async fn new_from(&self, _: &str, _: Arc<Handle>) -> Result<ReturnHandle, Error> { Err(Error::UnsupportedProtocol) }

	async fn read(&self, _handle: isize, count: usize) -> Result<Box<[u8]>, Error> {
		let mut buf = Box::new_uninit_slice(count);
		for i in 0..count {
			buf[i].write(crate::hal::SerialOut::read());
		}
		Ok(unsafe { buf.assume_init() })
	}
}

#[derive(Default)]
pub struct CtorCtx;

impl CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> {
		static VISITORS: OnceLock<ProtocolVisitor<CtorCtx>> = OnceLock::new();
		
		VISITORS.get_or_init(||
			ProtocolVisitor::new()
					.add_visitor::<dyn protocol::generated::CoreIoRead>(|_, _| Ok(()))
					.add_visitor::<dyn protocol::generated::CoreIoWrite>(|_, _| Ok(()))
		)
	}
}
