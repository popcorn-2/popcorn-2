use alloc::sync::Arc;
use core::future::Future;
use core::sync::atomic::{AtomicIsize, Ordering};
use hashbrown::HashMap;
use kernel_api::ptr::LocalUser;
use kernel_api::sync::{OnceLock, Spinlock};
use kernel_api::syscall;
use utils::better_cow::Cow;
use crate::hashmap_new;
use crate::ipc::{Error, protocol, server};
use crate::ipc::ctor::{CtorContext, ProtocolVisitor};
use kernel_api::syscall::handle::Handle;
use kernel_api::syscall::server::ServerId;
use crate::ipc::protocol::DispatchTable;
use crate::ipc::server::{ReturnHandle, Server, ServerTy};
use crate::ipc::server::userspace::{Packet, Response, UserspaceServer};

#[derive(Debug)]
pub struct RootServer {
	next_handle: AtomicIsize,
	handle_map: Spinlock<HashMap<isize, ServerId>>,
}

impl RootServer {
	pub const fn new() -> Self {
		Self {
			next_handle: AtomicIsize::new(0),
			handle_map: Spinlock::new(hashmap_new!()),
		}
	}
}

impl Server for RootServer {
	type CtorContext = CtorCtx;

	async fn ctor(&self, endpoint: &str, _ctx: Self::CtorContext) -> Result<ReturnHandle, Error> {
		let path = endpoint.trim_start_matches('/');
		if path.contains('/') { yeet!(Error::InvalidEndpoint); }
		
		debug!("start server at `{path}`");

		let handle = {
			if self.next_handle.load(Ordering::Relaxed) == isize::MAX { panic!("overflow"); } // todo: better
			let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
			handle
		};

		let new_server = ServerTy::Userspace(UserspaceServer::new_current_thread());
		
		let name = if path.is_empty() { None }
			else { Some(Cow::Owned(endpoint.into())) };
		let id = server::server_registry_mut().insert_server(name, new_server)?; // fixme: silly allocation

		self.handle_map.lock().try_insert(handle, id)
		    .expect("Handle reuse should not happen");

		Ok(ReturnHandle::NewDefault(handle))
	}

	async fn destroy(&self, _handle: isize) -> Result<(), Error> { Ok(()) }

	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH_TABLE: OnceLock<DispatchTable> = OnceLock::new();
		DISPATCH_TABLE.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as protocol::generated::core::server::Sync>::__vtable())
		)
	}
}

impl protocol::generated::core::server::Sync for RootServer {
	async fn new_from(&self, _: &str, _: Arc<Handle>) -> Result<ReturnHandle, Error> { Err(Error::UnsupportedProtocol) }

	// fixme: the buffer ptr is actually a User<*mut u8>
	fn next(&self, handle: isize, buffer: *const u8) -> impl Future<Output = syscall::Result<()>> {
		let buffer = buffer.addr();
		let server: syscall::Result<_> = try {
			let Some(&server) = self.handle_map.lock().get(&handle) else {
				Err(Error::InvalidHandle)?;
				unreachable!()
			};
			let Ok(server) = server::server_registry_mut().get_server(server) else {
				self.handle_map.lock().remove(&handle);
				Err(Error::DeadServer)?;
				unreachable!()
			};

			server
		};
		
		async move {
			let server = server?;
			let userspace = match &*server {
				ServerTy::Userspace(server) => server,
				_ => unreachable!("root server should not contain kernel servers"),
			};

			let packet = userspace.get_packet().await;
			
			let buffer = LocalUser::<*mut Packet>::new(buffer);
			buffer.write(packet)?;

			Ok(())
		}
	}

	// fixme: the buffer ptr is actually a User<*const u8>
	fn reply(&self, handle: isize, buffer: *const u8) -> impl Future<Output = syscall::Result<()>> {
		let res = try {
			let buffer = unsafe { LocalUser::<*const Response>::new(buffer.addr()) };

			let packet = buffer.read().map_err(From::from)?;

			let Some(&server) = self.handle_map.lock().get(&handle) else {
				Err(Error::InvalidHandle)?;
				unreachable!()
			};
			let Ok(server) = server::server_registry_mut().get_server(server) else {
				self.handle_map.lock().remove(&handle);
				Err(Error::DeadServer)?;
				unreachable!()
			};

			let userspace = match &*server {
				ServerTy::Userspace(server) => server,
				_ => unreachable!("root server should not contain kernel servers"),
			};

			userspace.reply_packet(packet)?;
		};

		core::future::ready(res)
	}

	async fn forge(&self, handle: isize, handle_num: isize, protocols: &[u128]) -> Result<ReturnHandle, Error> {
		let Some(&server) = self.handle_map.lock().get(&handle) else {
			return Err(Error::InvalidHandle);
		};
		
		debug!("forge handle for server {server:?} with num {handle_num:#x} and protos {protocols:#x?}");
		
		let new_handle = Handle::new(server, handle_num, protocols, "[forged]");

		Ok(ReturnHandle::Transfer(new_handle))
	}
}

#[derive(Default)]
pub struct CtorCtx;

impl CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> {
		static VISITORS: OnceLock<ProtocolVisitor<CtorCtx>> = OnceLock::new();

		VISITORS.get_or_init(||
			ProtocolVisitor::new()
				.add_visitor::<dyn protocol::generated::core::server::Sync>(|_, _| Ok(()))
		)
	}
}
