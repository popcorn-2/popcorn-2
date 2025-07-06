use alloc::sync::Arc;
use crate::prelude::*;
use core::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use hashbrown::HashMap;
use kernel_api::ptr::User;
use kernel_api::sync::{OnceLock, Spinlock};
use utils::better_cow::Cow;
use crate::{hashmap_new, ipc};
use crate::ipc::{Error, protocol, server};
use crate::ipc::ctor::{CtorContext, ProtocolVisitor};
use crate::ipc::handle::{Handle, ServerId};
use crate::ipc::protocol::DispatchTable;
use crate::ipc::server::{ReturnHandle, Server, ServerTy};
use crate::ipc::server::userspace::{Packet, Response, UserspaceServer};
use crate::memory::r#virtual::AddressSpaceInner;

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

	fn ctor(&self, endpoint: &str, _ctx: Self::CtorContext) -> Result<ReturnHandle, Error> {
		let path = endpoint.trim_start_matches('/');
		if path.contains('/') { yeet!(Error::InvalidName); }
		
		debug!("start server at `{path}`");

		let handle = {
			if self.next_handle.load(Ordering::Relaxed) == isize::MAX { panic!("overflow"); } // todo: better
			let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
			handle
		};

		let new_server = ServerTy::Userspace(UserspaceServer::new_current_thread());
		let id = server::server_registry_mut().insert_server(Some(Cow::Owned(endpoint.into())), new_server)?; // fixme: silly allocation

		self.handle_map.lock().try_insert(handle, id)
		    .expect("Handle reuse should not happen");

		Ok(ReturnHandle::NewDefault(handle))
	}

	fn destroy(&self, _handle: isize) -> Result<(), Error> { Ok(()) }

	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH_TABLE: OnceLock<DispatchTable> = OnceLock::new();
		DISPATCH_TABLE.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as protocol::generated::CoreServerSync>::__vtable())
		)
	}
}

impl protocol::generated::CoreServerSync for RootServer {
	fn new_from(&self, _: &str, _: Arc<Handle>) -> Result<ReturnHandle, Error> { Err(Error::UnsupportedProtocol) }

	// fixme: the buffer ptr is actually a User<*mut u8>
	fn next(&self, handle: isize, buffer: *const u8) -> Result<(), Error> {
		let buffer = User::<*mut u8>::new_in(
			buffer.cast_mut(),
			AddressSpaceInner::to_api(
				percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
				                          .tcb_ref().address_space
			),
		);
		
		let Some(&server) = self.handle_map.lock().get(&handle) else {
			return Err(Error::InvalidHandle);
		};
		let Ok(server) = server::server_registry_mut().get_server(server) else {
			self.handle_map.lock().remove(&handle);
			return Err(Error::DeadServer);
		};

		let userspace = match &*server {
			ServerTy::Userspace(server) => server,
			_ => unreachable!("root server should not contain kernel servers"),
		};
		
		let packet = userspace.get_packet_blocking();
		
		buffer.cast::<Packet>().copy_from_nonoverlapping(&packet, 1)?;
		
		Ok(())
	}

	// fixme: the buffer ptr is actually a User<*const u8>
	fn reply(&self, handle: isize, buffer: *const u8) -> Result<(), Error> {
		let buffer = User::<*const u8>::new_in(
			buffer,
			AddressSpaceInner::to_api(
				percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
				                          .tcb_ref().address_space
			),
		);
		let packet = unsafe { buffer.cast::<Response>().read() }?;

		let Some(&server) = self.handle_map.lock().get(&handle) else {
			return Err(Error::InvalidHandle);
		};
		let Ok(server) = server::server_registry_mut().get_server(server) else {
			self.handle_map.lock().remove(&handle);
			return Err(Error::DeadServer);
		};

		let userspace = match &*server {
			ServerTy::Userspace(server) => server,
			_ => unreachable!("root server should not contain kernel servers"),
		};
		
		userspace.reply_packet(packet)?;
		
		Ok(())
	}

	fn forge(&self, handle: isize, handle_num: isize, protocols: &[u128]) -> Result<ReturnHandle, Error> {
		let Some(&server) = self.handle_map.lock().get(&handle) else {
			return Err(Error::InvalidHandle);
		};
		
		debug!("forge handle for server {server:?} with num {handle_num:#x} and protos {protocols:#x?}");
		
		let new_handle = Handle::new(server, handle_num, protocols);

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
				.add_visitor::<dyn protocol::generated::CoreServerSync>(|_, _| Ok(()))
		)
	}
}
