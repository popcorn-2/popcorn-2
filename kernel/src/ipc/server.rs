use alloc::boxed::Box;
use alloc::sync::Arc;
use core::cmp::min;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU16, Ordering};
use hashbrown::HashMap;
use kernel_api::sync::{LazyLock, RwSpinlock};
use utils::better_cow::Cow;
use crate::ipc::ctor::{CtorArgs, CtorContext};
use crate::ipc::{dispatch, Error};
use crate::ipc::handle::{Handle, ServerId};
use crate::ipc::protocol::DispatchTable;
use crate::ipc::dispatch::{Arg, Return};

mod console;
mod proc;
pub(super) mod ramdisk;
mod root;
mod userspace;

pub trait Server {
	type CtorContext: CtorContext;

	fn ctor(&self, endpoint: &str, ctx: Self::CtorContext) -> Result<ReturnHandle, Error>;
	fn destroy(&self, handle: isize) -> Result<(), Error>;
	fn dispatch_table(&self) -> &'static DispatchTable;

	fn dispatch(
		&self,
		protocol: u128,
		method: u32,
		arg0: usize,
		arg1: usize,
		arg2: usize,
		arg3: usize,
		arg4: usize
	) -> Result<MethodResult, Error> {
		let dispatch_table = self.dispatch_table();
		dispatch_table.dispatch(
			protocol,
			method,
			self as *const _ as *const (),
			arg0, arg1, arg2, arg3, arg4
		)
	}
}

#[derive(Debug)]
pub enum ServerTy {
	Console(console::ConsoleServer),
	Proc(proc::ProcServer),
	Ramdisk(ramdisk::RamdiskServer),
	Root(root::RootServer),
	Userspace(userspace::UserspaceServer),
}

pub enum MethodResult {
	SelfHandle(isize, Box<[u128]>),
	SelfDefaultHandle(isize),
	TransferHandle(Arc<Handle>),
	Value(u128),
}

pub enum ReturnHandle {
	/// Transfers ownership of a handle to the caller, with support for all the protocols that the original handle had
	Transfer(Arc<Handle>),
	/// Creates a handle to a new object, with support for the protocols passed in
	New(isize, Box<[u128]>),
	/// Creates a handle to a new object, with support for all the protocols that were requested by the caller
	///
	/// Only valid to return from a constructor function.
	/// Panics otherwise.
	NewDefault(isize),
}

impl From<ReturnHandle> for MethodResult {
	fn from(value: ReturnHandle) -> Self {
		match value {
			ReturnHandle::Transfer(handle) => MethodResult::TransferHandle(handle),
			ReturnHandle::New(internal_id, protos) => MethodResult::SelfHandle(internal_id, protos),
			ReturnHandle::NewDefault(internal_id) => MethodResult::SelfDefaultHandle(internal_id),
		}
	}
}

impl ServerTy {
	pub fn ctor(&self, endpoint: &str, mut args: CtorArgs) -> Result<ReturnHandle, Error> {
		match self {
			Self::Console(s) => {
				let mut ctx = <console::ConsoleServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx)
			}
			Self::Proc(s) => {
				let mut ctx = <proc::ProcServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx)
			}
			Self::Ramdisk(s) => {
				let mut ctx = <ramdisk::RamdiskServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx)
			}
			Self::Root(s) => {
				let mut ctx = <root::RootServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx)
			}
			Self::Userspace(s) => s.ctor(endpoint, args),
		}
	}
	
	pub fn destroy(&self, handle: isize) -> Result<(), Error> {
		match self {
			Self::Console(s) => s.destroy(handle),
			Self::Proc(s) => s.destroy(handle),
			Self::Ramdisk(s) => s.destroy(handle),
			Self::Root(s) => s.destroy(handle),
			Self::Userspace(s) => s.destroy(handle),
		}
	}
	
	pub fn dispatch_table(&self) -> &'static DispatchTable {
		match self {
			Self::Console(s) => s.dispatch_table(),
			Self::Proc(s) => s.dispatch_table(),
			Self::Ramdisk(s) => s.dispatch_table(),
			Self::Root(s) => s.dispatch_table(),
			Self::Userspace(_) => unimplemented!(),
		}
	}
	
	pub fn dispatch(
		&self,
		protocol: u128,
		method: u32,
		mut serialized: dispatch::DeserializedArgs,
	) -> Result<Return, Error> {
		if let ServerTy::Userspace(s) = self {
			return s.dispatch(protocol, method, serialized);
		}

		let mut return_buffer = if let Some(size) = serialized.return_size {
			unsafe { Box::<[u8]>::new_zeroed_slice(size).assume_init() }
		} else { Box::from([]) };

		let buffer = serialized.buffer.as_mut_ptr();
		let args = serialized.args.map(|arg| match arg {
			Arg::Primitive(val) => val,
			Arg::BufferOffset(offset) => unsafe { buffer.byte_add(offset) }.addr(),
			Arg::NewBuffer => return_buffer.as_mut_ptr().addr(),
			Arg::Handle(handle) => Arc::into_raw(handle).addr(),
		});

		let val = match self {
			ServerTy::Console(s) => s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4]),
			ServerTy::Proc(s) => s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4]),
			ServerTy::Ramdisk(s) => s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4]),
			ServerTy::Root(s) => s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4]),
			ServerTy::Userspace(_) => unreachable!(),
		}?;

		if let Some(return_size) = serialized.return_size {
			let MethodResult::Value(val) = val else {
				unreachable!("kernel server returned non-primitive return for memory return type")	
			};
			let return_size = min(return_size, val as usize);
			let slice = &return_buffer[..return_size];
			Ok(Return::Boxed(Box::from(slice)))
		} else {
			Ok(Return::from(val))
		}
	}
}

static SERVERS: LazyLock<RwSpinlock<ServerRegistry>> = LazyLock::new(|| RwSpinlock::new(ServerRegistry::new()));

pub fn server_registry() -> impl Deref<Target = ServerRegistry> { SERVERS.read() }
pub(super) fn server_registry_mut() -> impl DerefMut<Target = ServerRegistry> { SERVERS.write() }

#[derive(Debug)]
pub struct ServerRegistry {
	next_id: AtomicU16,

	// TODO: namespacing
	// fixme: privacy
	pub(super) name_lookup: HashMap<Cow<'static, Box<str>, str>, ServerId>,
	pub(super) server_map: HashMap<ServerId, Arc<ServerTy>>,
}

impl ServerRegistry {
	fn new() -> Self {
		let mut list = Self {
			next_id: AtomicU16::new(1),
			name_lookup: HashMap::new(),
			server_map: HashMap::new(),
		};

		list.insert_server(Some(Cow::Borrowed("")), ServerTy::Root(root::RootServer::new()))
		    .expect("Not enough servers inserted yet for overflow");

		let proc = list.insert_server(Some(Cow::Borrowed("proc")), ServerTy::Proc(proc::ProcServer::new()))
		    .expect("Not enough servers inserted yet for overflow");
		
		list.name_lookup.try_insert(Cow::Borrowed("elf"), proc).expect("`elf` shouldn't exist");

		list.insert_server(Some(Cow::Borrowed("console")), ServerTy::Console(console::ConsoleServer::new()))
		    .expect("Not enough servers inserted yet for overflow");

		list
	}

	pub(super) fn new_id(&self) -> Result<ServerId, Error> {
		if usize::from(self.next_id.load(Ordering::Relaxed)) == ServerId::MAX { panic!("overflow"); } // todo: better

		let id = self.next_id.fetch_add(1, Ordering::Relaxed);
		Ok(ServerId::new(id))
	}

	pub(super) fn insert_server(&mut self, name: Option<Cow<'static, Box<str>, str>>, server: ServerTy) -> Result<ServerId, Error> {
		let id = self.new_id()?;

		if let Some(name) = name {
			self.name_lookup.try_insert(name.into(), id)
			    .map_err(|_| Error::NameInUse)?;
		}

		self.server_map.try_insert(id, Arc::new(server))
		    .expect("Newly generated handle shouldn't be in use");

		Ok(id)
	}

	pub fn get_server_at(&self, name: &str) -> Result<(ServerId, Arc<ServerTy>), Error> {
		let id = self.name_lookup.get(name)
		             .ok_or(Error::EndpointNotFound)?;
		match self.get_server(*id) {
			Ok(srv) => Ok((*id, srv)),
			Err(err) => Err(err),
		}
	}

	pub fn get_server(&self, id: ServerId) -> Result<Arc<ServerTy>, Error> {
		let srv = self.server_map.get(&id)
		              .ok_or(Error::EndpointNotFound)?;
		Ok(Arc::clone(srv))
	}
}

