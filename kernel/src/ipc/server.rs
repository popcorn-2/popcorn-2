#[allow(unused_imports)] use crate::prelude::*;
use alloc::sync::Arc;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use enum_dispatch::enum_dispatch;
use hashbrown::HashMap;
use kernel_api::sync::{LazyLock, OnceLock, RwSpinlock};
use kernel_api::time::Instant;
use utils::better_cow::Cow;
use crate::ipc::{Error, NonNegativeIsize};

mod root;
mod userspace;
mod proc;
mod console;

use root::RootServer;
use userspace::UserspaceServer;
use proc::ProcServer;
use console::ConsoleServer;

#[enum_dispatch(ServerTy)]
pub trait Server {
	fn open(&self, endpoint: Cow<'_, Box<str>, str>) -> Result<usize, Error>;
	fn dispatch_vvv_ve(&self, proto_method: u128, fd: usize, b: usize, c: usize, d: usize) -> Result<NonNegativeIsize, Error> { Err(Error::Unimplemented) }
	fn dispatch_vm_ve(&self, proto_method: u128, fd: usize, b: usize, m: Box<[u8]>) -> Result<NonNegativeIsize, Error> { Err(Error::Unimplemented) }
	fn dispatch_vs_ve(&self, proto_method: u128, fd: usize, b: usize, s: String) -> Result<NonNegativeIsize, Error> { Err(Error::Unimplemented) }
	fn dispatch_vM_ve(&self, proto_method: u128, fd: usize, b: usize, size: usize) -> Result<(Box<[u8]>, NonNegativeIsize), Error> { Err(Error::Unimplemented) }
	fn dispatch_vS_ve(&self, proto_method: u128, fd: usize, b: usize, size: usize) -> Result<(String, NonNegativeIsize), Error> { Err(Error::Unimplemented) }
}

#[enum_dispatch]
#[derive(Debug)]
pub enum ServerTy {
	RootServer,
	UserspaceServer,
	ProcServer,
	ConsoleServer,
}

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub struct ServerId(usize, u64);

impl ServerId {
	const MAX: usize = u16::MAX as usize;
	
	pub fn get(self) -> u16 {
		debug_assert!(self.0 <= Self::MAX, "ServerId should not be above ServerId::MAX");
		self.0 as u16
	}
	
	pub fn new(val: u16) -> Self { Self(val.into(), Instant::now().get() as u64) } // assuming arch val is clock cycles, still takes >100 years to overflow generation number creating a new server every clock cycle at 5GHz
}

static SERVERS: LazyLock<RwSpinlock<ServerList>> = LazyLock::new(|| RwSpinlock::new(ServerList::new()));

pub fn servers() -> impl Deref<Target = ServerList> { SERVERS.read() }
pub(super) fn servers_mut() -> impl DerefMut<Target = ServerList> { SERVERS.write() }

#[derive(Debug)]
pub struct ServerList {
	next_id: AtomicU16,

	// TODO: namespacing
	// fixme: privacy
	pub(super) name_lookup: HashMap<Cow<'static, Box<str>, str>, ServerId>,
	pub(super) server_map: HashMap<ServerId, Arc<ServerTy>>,
}

impl ServerList {
	fn new() -> Self {
		let mut list = Self {
			next_id: AtomicU16::new(1),
			name_lookup: HashMap::new(),
			server_map: HashMap::new(),
		};

		list.insert_server(Some(Cow::Borrowed("")), RootServer::new().into())
				.expect("Not enough servers inserted yet for overflow");
		
		list.insert_server(Some(Cow::Borrowed("proc")), ProcServer::new().into())
				.expect("Not enough servers inserted yet for overflow");

		list.insert_server(Some(Cow::Borrowed("console")), ConsoleServer::new().into())
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
				.ok_or(Error::BadServer)?;
		match self.get_server(*id) {
			Ok(srv) => Ok((*id, srv)),
			Err(err) => Err(err),
		}
	}

	pub fn get_server(&self, id: ServerId) -> Result<Arc<ServerTy>, Error> {
		let srv = self.server_map.get(&id)
		              .ok_or(Error::BadServer)?;
		Ok(Arc::clone(srv))
	}
}
