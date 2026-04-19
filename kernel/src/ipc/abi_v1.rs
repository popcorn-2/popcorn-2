use alloc::sync::Arc;
use core::ptr;
use core::str::pattern::{Pattern, Searcher};
use kernel_api::executor::block_on;
use kernel_api::ptr::{local_slice_from_raw_parts, LocalUser};
use kernel_api::syscall;
use kernel_api::syscall::handle::Handle;
use kernel_api::syscall::server::ServerId;
use crate::ipc::{async_handling, Error, server};
use crate::ipc::ctor::CtorArgs;
use crate::ipc::serde::{MethodResult, Serializer};
use crate::ipc::server::{ReturnHandle, ServerTy};

pub const ABI_V1: u128 = 0;

pub const NEW: u32 = 1;
pub const HAS_PROTOCOL: u32 = 2;
pub const DESTROY: u32 = 3;
pub const DUP: u32 = 4;
pub const NEW_FROM: u32 = 5;
pub const ASYNC_WAIT: u32 = 6;
pub const COMBINE: u32 = 7;

pub const SYNC_ONLY: &[u32] = &[HAS_PROTOCOL, DUP, ASYNC_WAIT, COMBINE];

fn get_endpoint(ptr: usize, len: usize) -> syscall::Result<Box<str>> {
	let endpoint = {
		let endpoint_ptr = unsafe { LocalUser::<*const u8>::new(ptr) };
		unsafe { local_slice_from_raw_parts(endpoint_ptr, len) }
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
			return Err(Error::InvalidUtf8);
		},
	};

	Ok(endpoint)
}

pub fn new(
	arg0: usize,
	arg1: usize,
	arg2: usize,
	arg3: usize,
	_arg4: usize,
) -> syscall::Result<impl Future<Output = (syscall::Result<MethodResult>, Serializer)>> {
	let endpoint = get_endpoint(arg0, arg1)?;

	let protocols = {
		let protocol_ptr = unsafe { LocalUser::<*const u128>::new(arg2) };
		unsafe { local_slice_from_raw_parts(protocol_ptr, arg3) }
	};
	trace!("protocols: {protocols:#p}");

	let protocols = protocols.read_to_box()?;
	
	Ok(async move {
		let mut server = ServerId::INVALID;
		let res = try {
			let (srv_id, srv, endpoint) = open_server(&endpoint)?;
			let res = srv.ctor(endpoint, CtorArgs::new(&*protocols)).await?;
			server = srv_id;
			MethodResult::from(res)
		};
		
		(res, Serializer::new(server, protocols))
	})
}

pub async fn open_async(endpoint: &str, protocols: &[u128]) -> syscall::Result<Arc<Handle>> {
	let (srv_id, srv, endpoint) = open_server(endpoint)?;

	Ok(match srv.ctor(endpoint, CtorArgs::new(&*protocols)).await? {
		ReturnHandle::Transfer(handle) => handle,
		ReturnHandle::New(id, protos, endpoint) => Handle::new(
			srv_id,
			id,
			&*protos,
			endpoint,
		),
		ReturnHandle::NewDefault(id) => Handle::new(
			srv_id,
			id,
			protocols,
			endpoint,
		)
	})
}

fn open_server(endpoint: &str) -> syscall::Result<(ServerId, Arc<ServerTy>, &str)> {
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

pub fn has_protocol(
	arg0: usize,
	arg1: usize,
	arg2: usize,
	_arg3: usize,
	_arg4: usize,
) -> syscall::Result<u128> {
	let handle = arg0.try_into().map_err(|_| Error::InvalidHandle)?;
	
	let protocols = {
		let protocols = {
			let protocol_ptr = unsafe { LocalUser::<*const u128>::new(arg1) };
			unsafe { local_slice_from_raw_parts(protocol_ptr, arg2) }
		};
		trace!("protocols: {protocols:#p}");
		protocols.read_to_box()?
	};
	trace!("protocols: {protocols:#x?}");


	let res = percpu_v2!(current_thread)
			.read()
			.as_ref()
			.expect("must be running on thread to syscall")
			.handles
			.get(handle)?
			.has_protocols(&protocols);
	Ok(res as u128)
}

pub fn destroy(
	arg0: usize,
	_arg1: usize,
	_arg2: usize,
	_arg3: usize,
	_arg4: usize,
) -> syscall::Result<u128> {
	let handle = arg0.try_into().map_err(|_| Error::InvalidHandle)?;

	let _ = &percpu_v2!(current_thread)
			.read()
			.as_ref()
			.expect("must be running on thread to syscall")
			.handles
			.pop(handle)?;

	Ok(0)
}

pub fn dup(
	arg0: usize,
	_arg1: usize,
	_arg2: usize,
	_arg3: usize,
	_arg4: usize,
) -> syscall::Result<u128> {
	let handle = arg0.try_into().map_err(|_| Error::InvalidHandle)?;
	let guard = percpu_v2!(current_thread).read();
	let handles = &guard
			.as_ref()
			.expect("must be running on thread to syscall")
			.handles;
	Ok(handles.push(handles.get(handle)?)? as u128)
}

pub fn new_from(
	arg0: usize,
	arg1: usize,
	arg2: usize,
	arg3: usize,
	arg4: usize,
) -> syscall::Result<impl Future<Output = (syscall::Result<MethodResult>, Serializer)>> {
	let guard = percpu_v2!(current_thread).read();
	let handles = &guard
			.as_ref()
			.expect("must be running on thread to syscall")
			.handles;
	let handle = handles.pop(arg4.try_into().map_err(|_| Error::InvalidHandle)?)?;

	let endpoint = get_endpoint(arg0, arg1)?;
	let protocol  = (arg2 as u128) | (arg3 as u128) << 64;
	let (srv_id, srv, endpoint) = open_server(&*endpoint)?;

	use super::serde;
	let deserialized = serde::Deserialized::new_buffer_input(
		srv_id,
		[
			serde::InputArg::BufferOffset(0),
			serde::InputArg::RawValue(endpoint.len()),
			serde::InputArg::Handle(handle),
			serde::InputArg::RawValue(0),
			serde::InputArg::RawValue(0),
		],
		Box::<str>::from(endpoint).into_boxed_bytes(),
	);

	let fut = srv.dispatch(protocol, 0, deserialized)?;
	Ok(async move {
		let (res, mut serializer) = fut.await;
		serializer.set_default_protos(Box::<[_]>::from([protocol]));
		(res, serializer)
	})
}

pub fn async_wait(
	arg0: usize,
	arg1: usize,
	_arg2: usize,
	_arg3: usize,
	_arg4: usize,
) -> syscall::Result<u128> {
	block_on(async_handling::async_wait(arg0, arg1, 1))
}

pub fn combine(
	arg0: usize,
	arg1: usize,
	_arg2: usize,
	_arg3: usize,
	_arg4: usize,
) -> syscall::Result<u128> {
	let handle_a = arg0.try_into().map_err(|_| Error::InvalidHandle)?;
	let handle_b_key = arg1.try_into().map_err(|_| Error::InvalidHandle)?;
	let guard = percpu_v2!(current_thread).read();
	let handles = &guard
			.as_ref()
			.expect("must be running on thread to syscall")
			.handles;
	
	let handle_a = handles.get(handle_a)?;
	let handle_b = handles.get(handle_b_key)?;
	
	match handle_a.merge(&handle_b) {
		Ok(_) => {
			handles.pop(handle_b_key).expect("already checked this handle exists");
			Ok(0)
		},
		Err(e) => Err(e),
	}
}
