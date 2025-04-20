#[allow(unused_imports)] use crate::prelude::*;
use alloc::sync::Arc;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};
use enum_dispatch::enum_dispatch;
use hashbrown::HashMap;
use kernel_api::sync::{LazyLock, RwSpinlock};
use utils::better_cow::Cow;
use crate::ipc::Error;

mod root;
mod userspace;

use root::RootServer;
use userspace::UserspaceServer;

#[enum_dispatch(ServerTy)]
pub trait Server {
	fn open(&self, endpoint: Cow<'_, Box<str>, str>) -> Result<u16, Error>;
}

#[enum_dispatch]
#[derive(Debug)]
pub enum ServerTy {
	RootServer,
	UserspaceServer,
}

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug)]
pub struct ServerId(usize);

impl ServerId {
	const MAX: usize = u16::MAX as usize;
	
	pub fn get(self) -> u16 {
		debug_assert!(self.0 <= Self::MAX, "ServerId should not be above ServerId::MAX");
		self.0 as u16
	}
	
	pub fn new(val: u16) -> Self { Self(val.into()) }
}

static SERVERS: LazyLock<RwSpinlock<ServerList>> = LazyLock::new(|| RwSpinlock::new(ServerList::new()));

pub fn servers() -> impl Deref<Target = ServerList> { SERVERS.read() }
pub(super) fn servers_mut() -> impl DerefMut<Target = ServerList> { SERVERS.write() }

#[derive(Debug)]
pub struct ServerList {
	next_id: AtomicUsize,

	// TODO: namespacing
	// fixme: privacy
	pub(super) name_lookup: HashMap<Cow<'static, Box<str>, str>, ServerId>,
	pub(super) server_map: HashMap<ServerId, Arc<ServerTy>>,
}

impl ServerList {
	fn new() -> Self {
		let mut list = Self {
			next_id: AtomicUsize::new(1),
			name_lookup: HashMap::new(),
			server_map: HashMap::new(),
		};

		list.insert_server(Cow::Borrowed(""), RootServer::new().into())
				.expect("Not enough servers inserted yet for overflow");

		list
	}
	
	pub(super) fn new_id(&self) -> Result<ServerId, Error> {
		if self.next_id.load(Ordering::Relaxed) == ServerId::MAX { yeet!(Error::Overflow); }

		let id = self.next_id.fetch_add(1, Ordering::Relaxed);
		Ok(ServerId(id))
	}

	pub(super) fn insert_server(&mut self, name: Cow<'static, Box<str>, str>, server: ServerTy) -> Result<ServerId, Error> {
		let id = self.new_id()?;

		self.name_lookup.try_insert(name.into(), id)
				.map_err(|_| Error::NameInUse)?;

		self.server_map.try_insert(id, Arc::new(server))
				.expect("Newly generated handle shouldn't be in use");

		Ok(id)
	}

	pub fn get_server(&self, name: &str) -> Result<(ServerId, Arc<ServerTy>), Error> {
		let id = self.name_lookup.get(name)
				.ok_or(Error::BadServer)?;
		let srv = self.server_map.get(id)
			.ok_or(Error::BadServer)?;
		Ok((*id, srv.clone()))
	}
}
