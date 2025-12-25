use alloc::boxed::Box;
use alloc::sync::Arc;
use core::future::Future;
use core::ops::{Deref, DerefMut};
use core::pin::Pin;
use core::sync::atomic::{AtomicU16, Ordering};
use core::task::{Context, Poll};
use hashbrown::HashMap;
use log::trace;
use kernel_api::sync::{LazyLock, RwSpinlock, Syncify};
use kernel_api::syscall;
use kernel_api::syscall::Error;
use kernel_api::syscall::handle::Handle;
use kernel_api::syscall::server::ServerId;
use utils::better_cow::Cow;
use crate::ipc::ctor::{CtorArgs, CtorContext};
use crate::ipc::protocol::DispatchTable;
use crate::ipc::serde::{Deserialized, MethodResult, Serializer};

mod console;
pub(super) mod proc;
pub(super) mod ramdisk;
mod root;
mod userspace;
mod mem;

pub trait Server {
	type CtorContext: CtorContext;

	async fn ctor(&self, endpoint: &str, ctx: Self::CtorContext) -> syscall::Result<ReturnHandle>;
	async fn destroy(&self, handle: isize) -> syscall::Result<()>;
	fn dispatch_table(&self) -> &'static DispatchTable;

	fn dispatch<'a>(
		&'a self,
		protocol: u128,
		method: u32,
		arg0: usize,
		arg1: usize,
		arg2: usize,
		arg3: usize,
		arg4: usize
	) -> syscall::Result<Pin<Box<dyn Send + 'a + Future<Output = syscall::Result<MethodResult>>>>> where Self: Sized {
		let dispatch_table = self.dispatch_table();
		Ok(dispatch_table.dispatch(
			protocol,
			method,
			unsafe { &*(self as *const _ as *const ()) },
			arg0, arg1, arg2, arg3, arg4
		)?)
	}
}

#[derive(Debug)]
pub enum ServerTy {
	Console(console::ConsoleServer),
	Proc(proc::ProcServer),
	Ramdisk(ramdisk::RamdiskServer),
	Root(root::RootServer),
	Userspace(userspace::UserspaceServer),
	Mem(mem::MemServer),
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

enum Either<T, U> {
	Left(T),
	Right(U),
}

impl<'a, T: Future + 'a, U: Future<Output = T::Output> + 'a> Future for Either<T, U> {
	type Output = T::Output;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		match unsafe { self.get_unchecked_mut() } {
			Self::Left(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),
			Self::Right(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),
		}
	}
}

impl ServerTy {
	pub async fn ctor(&self, endpoint: &str, mut args: CtorArgs<'_>) -> syscall::Result<ReturnHandle> {
		match self {
			Self::Console(s) => {
				let mut ctx = <console::ConsoleServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx).await
			}
			Self::Proc(s) => {
				let mut ctx = <proc::ProcServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx).await
			}
			Self::Ramdisk(s) => {
				let mut ctx = <ramdisk::RamdiskServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx).await
			}
			Self::Root(s) => {
				let mut ctx = <root::RootServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx).await
			}
			Self::Mem(s) => {
				let mut ctx = <mem::MemServer as Server>::CtorContext::default();
				args.process_with(s, &mut ctx)?;
				s.ctor(endpoint, ctx).await
			}
			Self::Userspace(s) => s.ctor(endpoint, args).await,
		}
	}
	
	pub async fn destroy(&self, handle: isize) -> syscall::Result<()> {
		match self {
			Self::Console(s) => s.destroy(handle).await,
			Self::Proc(s) => s.destroy(handle).await,
			Self::Ramdisk(s) => s.destroy(handle).await,
			Self::Root(s) => s.destroy(handle).await,
			Self::Mem(s) => s.destroy(handle).await,
			Self::Userspace(s) => s.destroy(handle).await,
		}
	}
	
	pub fn dispatch(
		self: Arc<Self>,
		protocol: u128,
		method: u32,
		args: Deserialized,
	) -> Result<impl Future<Output = (Result<MethodResult, Error>, Serializer)> + 'static, Error> {
		let self_ptr = unsafe { Syncify::new(Arc::as_ptr(&self)) };
		let this = unsafe { &*Arc::into_raw(self) };
		
		if let ServerTy::Userspace(s) = this {
			return Ok(Either::Left(async move {
				let res = s.dispatch(protocol, method, args).await;
				unsafe { Arc::from_raw(self_ptr.into_inner()); }
				res
			}));
		}

		let (args, serializer) = args.into_args();

		trace!("args: {args:x?}");

		let fut = match this {
			ServerTy::Console(s) => {
				s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4])?
			},
			ServerTy::Proc(s) => {
				s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4])?
			},
			ServerTy::Ramdisk(s) => {
				s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4])?
			},
			ServerTy::Root(s) => {
				s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4])?
			},
			ServerTy::Mem(s) => {
				s.dispatch(protocol, method, args[0], args[1], args[2], args[3], args[4])?
			},
			ServerTy::Userspace(_) => unreachable!(),
		};

		Ok(Either::Right(async move {
			let res = fut.await;
			unsafe {
				let _ = Arc::from_raw(self_ptr.into_inner());
			}
			(res, serializer)
		}))
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

		list.insert_server(Some(Cow::Borrowed("console")), ServerTy::Console(console::ConsoleServer::new()))
		    .expect("Not enough servers inserted yet for overflow");
		
		list.insert_server(Some(Cow::Borrowed("mem")), ServerTy::Mem(mem::MemServer::new()))
		    .expect("Not enough servers inserted yet for overflow");

		list
	}

	pub(super) fn new_id(&self) -> syscall::Result<ServerId> {
		if usize::from(self.next_id.load(Ordering::Relaxed)) == ServerId::MAX { return Err(Error::Overflow); } // todo: better

		let id = self.next_id.fetch_add(1, Ordering::Relaxed);
		Ok(ServerId::new(id))
	}

	pub(super) fn insert_server(&mut self, name: Option<Cow<'static, Box<str>, str>>, server: ServerTy) -> syscall::Result<ServerId> {
		let id = self.new_id()?;

		if let Some(name) = name {
			self.name_lookup.try_insert(name.into(), id)
			    .map_err(|_| Error::NameInUse)?;
		}

		self.server_map.try_insert(id, Arc::new(server))
		    .expect("Newly generated handle shouldn't be in use");

		Ok(id)
	}

	pub fn get_server_at(&self, name: &str) -> syscall::Result<(ServerId, Arc<ServerTy>)> {
		let id = self.name_lookup.get(name)
		             .ok_or(Error::EndpointNotFound)?;
		match self.get_server(*id) {
			Ok(srv) => Ok((*id, srv)),
			Err(err) => Err(err),
		}
	}

	pub fn get_server(&self, id: ServerId) -> syscall::Result<Arc<ServerTy>> {
		let srv = self.server_map.get(&id)
		              .ok_or(Error::EndpointNotFound)?;
		Ok(Arc::clone(srv))
	}
}

