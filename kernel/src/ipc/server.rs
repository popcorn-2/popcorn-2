use alloc::boxed::Box;
use alloc::sync::Arc;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU16, Ordering};
use hashbrown::HashMap;
use kernel_api::sync::{LazyLock, RwSpinlock};
use utils::better_cow::Cow;
use crate::ipc::ctor::{CtorArgs, CtorContext};
use crate::ipc::Error;
use crate::ipc::handle::ServerId;
use crate::ipc::protocol::DispatchTable;

mod console;
mod proc;

pub trait Server {
	type CtorContext: CtorContext;

	fn ctor(&self, endpoint: &str, ctx: Self::CtorContext) -> Result<isize, Error>;
	fn destroy(&self, handle: isize) -> Result<(), Error>;
	fn dispatch_table(&self) -> &'static DispatchTable;
}

#[derive(Debug)]
pub enum ServerTy {
	Console(console::ConsoleServer),
	Proc(proc::ProcServer),
}

impl ServerTy {
	pub fn ctor(&self, endpoint: &str, mut args: CtorArgs) -> Result<isize, Error> {
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
		}
	}

	pub fn destroy(&self, handle: isize) -> Result<(), Error> {
		match self {
			Self::Console(s) => s.destroy(handle),
			Self::Proc(s) => s.destroy(handle),
		}
	}
	
	pub fn dispatch_table(&self) -> &'static DispatchTable {
		match self {
			Self::Console(s) => s.dispatch_table(),
			Self::Proc(s) => s.dispatch_table(),
		}
	}
	
	pub fn this(&self) -> *const () {
		match self {
			ServerTy::Console(ref s) => s as *const _ as *const _,
			ServerTy::Proc(ref s) => s as *const _ as *const _,
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

		/*list.insert_server(Some(Cow::Borrowed("")), root::RootServer::new().into())
		    .expect("Not enough servers inserted yet for overflow");*/

		list.insert_server(Some(Cow::Borrowed("proc")), ServerTy::Proc(proc::ProcServer))
		    .expect("Not enough servers inserted yet for overflow");

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

