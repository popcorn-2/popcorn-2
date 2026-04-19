use alloc::sync::Arc;
use core::future::Future;
use core::mem::ManuallyDrop;
use core::pin::Pin;
use core::task::{Context, Poll};
use itertools::Itertools;
use kernel_api::executor::{block_on, spawn};
use kernel_api::memory::PhysicalAddress;
use kernel_api::ptr::User;
use kernel_api::{dbg, syscall};
use kernel_api::syscall::Error;
use kernel_api::syscall::handle::Handle;
use kernel_api::syscall::server::ServerId;
use kernel_api::threading::ThreadMeta;
use utils::better_cow::Cow;
use crate::ipc::serde::{MethodResult, Serializer};
use crate::ipc::server::ServerTy;

pub mod protocol;
mod ctor;
pub mod server;
pub mod async_handling;
mod serde;
mod abi_v1;
mod executor;

pub trait HandleExt {
	async fn kernel_syscall(&self, protocol: u128, method: u32, args: [usize; 4]) -> syscall::Result<u128>;
}

impl HandleExt for Arc<Handle> {
	async fn kernel_syscall(&self, protocol: u128, method: u32, args: [usize; 4]) -> syscall::Result<u128> {
		let fut = handle_syscall_inner(
			self,
			protocol,
			method,
			args,
		);

		let (result, serializer) = fut?.await;
		serializer.serialize(result)
	}
}

#[unsafe(export_name = "__popcorn_ksyscall_blocking")]
fn kernel_syscall_blocking(
	this: &Arc<Handle>,
	protocol: u128,
	method: u32,
	args: [usize; 4]
) -> syscall::Result<u128> {
	let fut = this.kernel_syscall(protocol, method, args);
	block_on(fut)
}

fn handle_syscall_inner(
	this: &Arc<Handle>,
	protocol: u128,
	method: u32,
	args: [usize; 4]
) -> syscall::Result<impl Future<Output = (syscall::Result<MethodResult>, Serializer)> + use<>> {
	let deserialized = serde::deserialize(
		protocol,
		method,
		this,
		&args,
	)?;

	let srv = server::server_registry().get_server(deserialized.server())?;

	srv.dispatch(
		protocol,
		method,
		deserialized,
	)
}

#[inline]
pub fn entry(
	protocol: u128,
	method: u32,
	arg0: usize,
	arg1: usize,
	arg2: usize,
	arg3: usize,
	arg4: usize,
	async_key: Option<usize>,
) -> syscall::Result<u128> {
	let fut = if protocol == abi_v1::ABI_V1 {
		// todo: move into `serde::deserialize()`
		if async_key.is_some() && abi_v1::SYNC_ONLY.contains(&method) { return Err(Error::AsyncUnsupported); }

		match method {
			abi_v1::NEW => Either::First(abi_v1::new(arg0, arg1, arg2, arg3, arg4)?),
			abi_v1::DESTROY => return abi_v1::destroy(arg0, arg1, arg2, arg3, arg4),
			abi_v1::HAS_PROTOCOL => return abi_v1::has_protocol(arg0, arg1, arg2, arg3, arg4),
			abi_v1::DUP => return abi_v1::dup(arg0, arg1, arg2, arg3, arg4),
			abi_v1::NEW_FROM => Either::Second(abi_v1::new_from(arg0, arg1, arg2, arg3, arg4)?),
			abi_v1::ASYNC_WAIT => return abi_v1::async_wait(arg0, arg1, arg2, arg3, arg4),
			abi_v1::COMBINE => return abi_v1::combine(arg0, arg1, arg2, arg3, arg4),
			_ => return Err(Error::UnknownProtocol),
		}
	} else {
		let handle = arg0.try_into().map_err(|_| Error::InvalidHandle)?;
		let handle = percpu_v2!(current_thread)
				.read()
				.as_ref()
				.expect("must be running on thread to syscall")
				.handles
				.get(handle)?;

		let fut = handle_syscall_inner(
			&handle,
			protocol,
			method,
			[arg1, arg2, arg3, arg4]
		)?;

		Either::Third(fut)
	};

	if let Some(async_key) = async_key {
		let map = percpu_v2!(current_thread)
				.read()
				.as_ref()
				.expect("cannot syscall from not thread")
				.async_map.clone();

		spawn(async move {
			let (result, serializer) = fut.await;
			let result = serializer.serialize(dbg!(result));
			map.push_result(async_key, result);
		});

		Ok(async_key as u128)
	} else {
		let (result, serializer) = block_on(fut);
		serializer.serialize(dbg!(result))
	}
}

enum Either<T, U, V, /*W, X, Y, Z*/> {
	First(T),
	Second(U),
	Third(V),
	//Fourth(W),
	//Fifth(X),
	//Sixth(Y),
	//Seventh(Z),
}

impl<
	T: Future,
	U: Future<Output = T::Output>,
	V: Future<Output = T::Output>,
	//W: Future<Output = T::Output>,
	//X: Future<Output = T::Output>,
	//Y: Future<Output = T::Output>,
	//Z: Future<Output = T::Output>,
> Future for Either<T, U, V, /*W, X, Y, Z*/> {
	type Output = T::Output;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		match unsafe { self.get_unchecked_mut() } {
			Self::First(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),
			Self::Second(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),
			Self::Third(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),
			/*Self::Fourth(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),
			Self::Fifth(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),
			Self::Sixth(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),
			Self::Seventh(this) => unsafe { Pin::new_unchecked(this) }.poll(cx),*/
		}
	}
}

pub fn init_ramdisk(data: Box<[u8]>, rsdp: PhysicalAddress) -> ServerId {
	let ptr = rsdp.to_virtual().as_ptr();
	server::server_registry_mut().insert_server(
		None,
		ServerTy::Ramdisk(server::ramdisk::RamdiskServer::new(data, ptr)),
	).expect("failed to start ramdisk server")
}

pub fn init_proc(tid0: Arc<ThreadMeta>) {
	let proc = server::server_registry_mut().insert_server(
		Some(Cow::Borrowed("proc")),
		ServerTy::Proc(server::proc::ProcServer::new(tid0)),
	).expect("failed to start ramdisk server");

	server::server_registry_mut().name_lookup.try_insert(Cow::Borrowed("elf"), proc).expect("`elf` shouldn't exist");
}

pub fn open(endpoint: &str, protocols: &[u128], _ctor_args: User<'_, *const u8>) -> syscall::Result<Arc<Handle>> {
	block_on(abi_v1::open_async(endpoint, protocols))
}

#[unsafe(no_mangle)]
fn __popcorn_handle_drop(this: &mut Handle) {
	debug!("drop handle {this:#x?}");
	let map = unsafe { ManuallyDrop::take(this.__protocols.get_mut()) };
	spawn(async move {
		// since a handle can be tied to multiple servers, we need to send `destruct@abi_v1` to all servers
		// that back this handle, but we don't want to send a call with the same internal ID multiple times
		// as servers are allowed to assume that after a call to `destruct@abi_v1` the handle no longer exists
		let values = map.into_values()
		                .sorted()
		                .dedup();

		for (server_id, internal_id) in values {
			let Ok(srv) = server::server_registry().get_server(server_id) else { continue; };
			if let Err(e) = srv.destroy(internal_id).await {
				warn!("error {e:?} calling `destroy@abi_v1` on {internal_id}@{server_id:?}")
			}
		}
	})
}

/*fn async_entry(
	protocol: u128,
	method: u32,
	arg0: usize,
	arg1: usize,
	arg2: usize,
	arg3: usize,
	arg4: usize,
) -> Result<impl Future<Output = Result<u128, Error>> + Send + 'static, Error> {
	if protocol == abi_v1::ABI_V1 && (method == abi_v1::NEW || method == abi_v1::NEW_FROM) {
		// new@abi.v1 and new_from@abi.v1 - special cased

		let endpoint = {
			super let mut hazard = HazardPointer::new();
			let endpoint_ptr = unsafe { User::<*const u8>::new_current(arg0, &mut hazard) };
			slice_from_raw_parts(endpoint_ptr, arg1)
		};
		debug!("endpoint: {endpoint:#p}");

		let endpoint = endpoint.read_to_box()?;

		let endpoint = match core::str::from_utf8(&endpoint) {
			Ok(_) => {
				let ptr = Box::into_raw(endpoint);
				let ptr = ptr::from_raw_parts_mut(ptr.as_mut_ptr(), ptr.len());
				unsafe { Box::<str>::from_raw(ptr) }
			},
			Err(e) => {
				error!("{e:?}");
				yeet!(Error::InvalidUtf8);
			},
		};

		if method == abi_v1::NEW_FROM {
			let fut = abi_v1_new_from(endpoint, arg2, arg3, arg4)?;
			return Ok(Either::Second(async move {
				fut.await
			}));
		}

		let fut = abi_v1_new(endpoint, arg2, arg3, arg4)?;
		let handle_map = Arc::downgrade(
			percpu_v2!(current_thread).read().as_ref()
			                          .expect("must be running on a thread to syscall")
			                          .tcb_ref().handles
		);

		Ok(Either::First(async move {
			let handle = fut.await?;
			if let Some(handle_map) = handle_map.upgrade() {
				let res = handle_map.push(handle);
				res.map(|val| val as u128)
			} else { Err(Error::DeadThread) }
		}))
	} else if protocol == abi_v1::ABI_V1 && method == abi_v1::ASYNC_WAIT {
		Ok(Either::Sixth(async_handling::async_wait(arg0, arg1)))
	} else {
		let handle = percpu_v2!(current_thread).read().as_ref()
		                         .expect("must be running on a thread to syscall")
		                         .tcb_ref().handles.get(arg0.try_into().map_err(|_| Error::InvalidHandle)?)?;
		
		if protocol == abi_v1::ABI_V1 && method == abi_v1::HAS_PROTOCOL {
			// has_protocol@abi.v1 - special cased
			let protocols = {
				super let mut hazard = HazardPointer::new();
				let protocol_ptr = unsafe { User::<*const u128>::new_current(arg1, &mut hazard) };
				slice_from_raw_parts(protocol_ptr, arg2)
			};
			trace!("protocols: {protocols:#p}");
			let protocols = protocols.read_to_box()?;
			trace!("protocols: {protocols:#x?}");
			
			Ok(Either::Third(async move { Ok(handle.has_protocols(&*protocols) as u128) }))
		} else if protocol == abi_v1::ABI_V1 && method == abi_v1::DUP {
			// dup@abi.v1 - special cased
			let res = percpu_v2!(current_thread).read().as_ref()
			                                    .expect("must be running on a thread to syscall")
			                                    .tcb_ref().handles.push(handle);
			Ok(Either::Fourth(async move { res.map(|val| val as u128) }))
		} else if protocol == abi_v1::ABI_V1 && method == abi_v1::COMBINE {
			// combine@abi.v1 - special cased
			let second_handle_num = arg1.try_into().map_err(|_| Error::InvalidHandle)?;
			let guard = percpu_v2!(current_thread).read();
			let second_handle = guard.as_ref()
			                                       .expect("must be running on a thread to syscall")
			                                       .tcb_ref().handles.get(second_handle_num)?;
			
			let res = match handle.merge(&second_handle) {
				Ok(_) => {
					guard.as_ref()
					     .expect("must be running on a thread to syscall")
					     .tcb_ref().handles.pop(second_handle_num);
					Ok(0u128)
				},
				Err(e) => Err(e),
			};
			drop(guard);
			Ok(Either::Seventh(async move { res }))
		} else {
			Ok(Either::Fifth(kernel_syscall_entry(protocol, method, handle, arg1, arg2, arg3, arg4)?))
		}
	}
}

fn open_server(endpoint: &str) -> Result<(ServerId, Arc<ServerTy>, &str), Error> {
	let (domain, endpoint) = match <char as Pattern>::into_searcher(':', endpoint).next_match() {
		Some((begin, end)) => {
			(endpoint.get(..begin).unwrap(), endpoint.get(end..).unwrap())
		},
		None => ("", endpoint),
	};

	let endpoint = endpoint.trim_start_matches('/')
	                       .trim_end_matches('/');

	let domain = domain.trim_end_matches('.');

	info!("Open `{domain}:{endpoint}`");

	let (id, ty) = server::server_registry().get_server_at(domain)?;
	Ok((id, ty, endpoint))
}

fn abi_v1_new(endpoint: Box<str>, arg2: usize, arg3: usize, arg4: usize) -> Result<impl Future<Output = Result<Arc<Handle>, Error>> + Send + 'static, Error> {
	let protocols = {
		super let mut hazard = HazardPointer::new();
		let protocol_ptr = unsafe { User::<*const u128>::new_current(arg2, &mut hazard) };
		slice_from_raw_parts(protocol_ptr, arg3)
	};
	trace!("protocols: {protocols:#p}");

	let protocols = protocols.read_to_box()?;

	let ctor_args_ptr = unsafe {
		super let mut hazard = HazardPointer::new();
		User::<*const u8>::new_current(arg4, &mut hazard)
	};
	trace!("ctor_args: {ctor_args_ptr:#p}");

	Ok(async move {
		open_async(&*endpoint, &*protocols, ctor_args_ptr).await
	})
}

fn abi_v1_new_from(endpoint: Box<str>, arg2: usize, arg3: usize, arg4: usize) -> Result<impl Future<Output = Result<u128, Error>> + Send + 'static, Error> {
	let (handle_map, handle) = {
		let guard = percpu_v2!(current_thread).read();
		let handles = guard.as_ref()
		                   .expect("can only syscall from thread")
		                   .tcb_ref()
		                   .handles;
		let handle = handles.pop(arg4.try_into().map_err(|_| Error::InvalidHandle)?)?;
		let handle_map = Arc::downgrade(handles);
		(handle_map, handle)
	};

	let protocol = (arg2 as u128) | (arg3 as u128) << 64;
	let (srv_id, srv, endpoint) = open_server(&*endpoint)?;

	let args = DeserializedArgs {
		buffer: Box::<str>::from(endpoint).into_boxed_bytes(),
		args: [
			dispatch::Arg::BufferOffset(0),
			dispatch::Arg::Primitive(endpoint.len()),
			dispatch::Arg::Handle(handle),
			dispatch::Arg::Primitive(0),
			dispatch::Arg::Primitive(0),
		],
		return_size: None,
		return_ptr: RawUserPointer(0, 0),
		return_ty: ReturnArg::Handle,
	};

	let fut = srv.dispatch(
		protocol,
		0,
		args,
	)?;

	Ok(async move {
		let res = fut.await?;

		dispatch::serialize(
			res,
			RawUserPointer(0, 0),
			ReturnArg::Handle,
			Some(&[protocol]),
			srv_id,
			handle_map,
		).await
	})
}

async fn open_async(endpoint: &str, protocols: &[u128], ctor_args: User<'_, *const u8>) -> Result<Arc<Handle>, Error> {
	let (srv_id, srv, endpoint) = open_server(endpoint)?;

	Ok(match srv.ctor(endpoint, CtorArgs::new(&*protocols, ctor_args)).await? {
		ReturnHandle::Transfer(handle) => handle,
		ReturnHandle::New(id, protos) => Handle::new(
			srv_id,
			id,
			&*protos
		),
		ReturnHandle::NewDefault(id) => Handle::new(
			srv_id,
			id,
			protocols
		)
	})
}

pub fn kernel_syscall_entry(
	protocol: u128,
	method: u32,
	handle: Arc<Handle>,
	arg1: usize,
	arg2: usize,
	arg3: usize,
	arg4: usize,
) -> Result<impl Future<Output = Result<u128, Error>> + Send + 'static, Error> {
	let uid = protocol | (method as u128) << 96;
	let (server_id, internal_id) = handle.id(protocol)?;
	
	let args = [internal_id as usize, arg1, arg2, arg3, arg4];

	let serialized = dispatch::deserialize(uid, &args)?;
	let return_ptr = serialized.return_ptr;
	let return_ty = serialized.return_ty;
	let srv = server::server_registry().get_server(server_id)?;

	let handle_map = Arc::downgrade(
		percpu_v2!(current_thread).read().as_ref()
		                          .expect("must be running on a thread to syscall")
		                          .tcb_ref().handles
	);

	let fut = srv.dispatch(
		protocol,
		method,
		serialized,
	)?;

	Ok(async move {
		let res = fut.await?;

		dispatch::serialize(
			res,
			return_ptr,
			return_ty,
			None,
			server_id,
			handle_map,
		).await
	})
}*/
