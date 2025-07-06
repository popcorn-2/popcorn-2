use alloc::sync::Arc;
use core::cmp::min;
use core::str::pattern::{Pattern, Searcher};
use hashbrown::HashMap;
use kernel_api::memory::AllocError;
use crate::prelude::*;
use crate::memory::r#virtual::AddressSpaceInner;
use kernel_api::ptr::{PointerError, slice_from_raw_parts, slice_from_raw_parts_mut, User};
use kernel_api::sync::OnceLock;
use utils::better_cow::Cow;
use crate::ipc::ctor::CtorArgs;
use crate::ipc::dispatch::{Return, DeserializedArgs};
use crate::ipc::handle::{Handle, ServerId};
use crate::ipc::protocol::meta;
use crate::ipc::protocol::meta::{Meta, ReturnArg};
use crate::ipc::server::{ReturnHandle, ServerTy};

pub mod handle;
pub mod protocol;
mod ctor;
pub mod server;
mod dispatch;

#[derive(Debug)]
#[repr(u16)]
pub enum Error {
	InvalidPointer,
	InvalidUtf8,
	UnsupportedProtocol,
	UnknownProtocol,
	EndpointNotFound,
	NameInUse,
	InvalidHandle,
	Overflow,
	InvalidName,
	DeadServer,
	InProgress,
	Invalid,
	AllocationFailure,
	InvalidArg,
}

impl From<PointerError> for Error {
	fn from(_: PointerError) -> Self {
		Self::InvalidPointer
	}
}

impl From<AllocError> for Error {
	fn from(_: AllocError) -> Self {
		Self::AllocationFailure
	}
}

impl From<u128> for Error {
	fn from(value: u128) -> Self {
		match value {
			0 => Error::InvalidPointer,
			1 => Error::InvalidUtf8,
			2 => Error::UnsupportedProtocol,
			3 => Error::UnknownProtocol,
			4 => Error::EndpointNotFound,
			5 => Error::NameInUse,
			6 => Error::InvalidHandle,
			7 => Error::Overflow,
			8 => Error::InvalidName,
			9 => Error::DeadServer,
			10 => Error::InProgress,
			11 => Error::Invalid,
			12 => Error::AllocationFailure,
			_ => Error::Invalid,
		}
	}
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
) -> Result<u128, Error> {
	if protocol == 0 && (method == 1 || method == 5) {
		// new@abi.v1 and new_from@abi.v1 - special cased

		let endpoint = {
			let endpoint_ptr = User::<*const u8>::new_in(
				arg0 as _,
				AddressSpaceInner::to_api(
					percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
					                          .tcb_ref().address_space
				),
			);
			slice_from_raw_parts(endpoint_ptr, arg1)
		};
		debug!("endpoint: {endpoint:#p}");

		let Ok(endpoint) = (unsafe { endpoint.read_to_buffer() }) else { yeet!(Error::InvalidPointer); };

		let endpoint = match ::core::str::from_utf8(&endpoint) {
			Ok(endpoint) => endpoint,
			Err(e) => {
				error!("{e:?}");
				yeet!(Error::InvalidUtf8);
			},
		};

		if method == 5 {
			return abi_v1_new_from(endpoint, arg2, arg3, arg4);
		}

		let handle = abi_v1_new(endpoint, arg2, arg3, arg4)?;

		let res = percpu_v2!(current_thread).read().as_ref()
		                                    .expect("must be running on a thread to syscall")
		                                    .tcb_ref().handles.push(handle);
		res.map(|val| val as u128)
	} else {
		let handle = percpu_v2!(current_thread).read().as_ref()
		                         .expect("must be running on a thread to syscall")
		                         .tcb_ref().handles.get(arg0.try_into().map_err(|_| Error::InvalidHandle)?)?;
		
		if protocol == 0 && method == 2 {
			// has_protocol@abi.v1 - special cased

			let protocols = {
				let protocol_ptr = User::<*const u128>::new_in(
					arg1 as _,
					AddressSpaceInner::to_api(
						percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
						                          .tcb_ref().address_space
					),
				);
				slice_from_raw_parts(protocol_ptr, arg2)
			};
			debug!("protocols: {protocols:#p}");
			let protocols = unsafe { protocols.read_to_buffer() }?;
			debug!("protocols: {protocols:#x?}");
			
			Ok(handle.has_protocols(&*protocols) as u128)
		} else if protocol == 0 && method == 4 {
			// dup@abi.v1 - special cased

			let res = percpu_v2!(current_thread).read().as_ref()
			                                    .expect("must be running on a thread to syscall")
			                                    .tcb_ref().handles.push(handle);
			res.map(|val| val as u128)
		} else {
			let uid = protocol | (method as u128) << 96;
			let args = [handle.internal_id() as usize, arg1, arg2, arg3, arg4];

			let serialized = dispatch::deserialize(uid, &args)?;
			let return_ptr = serialized.return_ptr;
			let return_ty = serialized.return_ty;
			let srv = server::server_registry().get_server(handle.server_id())?;
			
			let res = srv.dispatch(
				protocol,
				method,
				serialized,
			)?;

			dispatch::serialize(
				res,
				return_ptr,
				return_ty,
				None,
				handle.server_id()
			)
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

fn abi_v1_new(endpoint: &str, arg2: usize, arg3: usize, arg4: usize) -> Result<Arc<Handle>, Error> {
	let protocols = {
		let protocol_ptr = User::<*const u128>::new_in(
			arg2 as _,
			AddressSpaceInner::to_api(
				percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
				                          .tcb_ref().address_space
			),
		);
		slice_from_raw_parts(protocol_ptr, arg3)
	};
	debug!("protocols: {protocols:#p}");

	let Ok(protocols) = (unsafe { protocols.read_to_buffer() }) else { yeet!(Error::InvalidPointer); };

	let ctor_args_ptr = User::<*const u8>::new_in(
		arg4 as _,
		AddressSpaceInner::to_api(
			percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
			                          .tcb_ref().address_space
		),
	);
	debug!("ctor_args: {ctor_args_ptr:#p}");

	open(endpoint, &*protocols, ctor_args_ptr)
}

fn abi_v1_new_from(endpoint: &str, arg2: usize, arg3: usize, arg4: usize) -> Result<u128, Error> {
	let handle = percpu_v2!(current_thread).read().as_ref()
	                                       .expect("can only syscall from thread")
	                                       .tcb_ref().handles.pop(arg4.try_into().map_err(|_| Error::InvalidHandle)?)?;

	let protocol = (arg2 as u128) | (arg3 as u128) << 64;
	let (srv_id, srv, endpoint) = open_server(endpoint)?;

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
		return_ptr: slice_from_raw_parts_mut(User::null_mut(), 0),
		return_ty: ReturnArg::Handle,
	};

	let res = srv.dispatch(
		protocol,
		0,
		args,
	)?;

	dispatch::serialize(
		res,
		slice_from_raw_parts_mut(User::null_mut(), 0),
		ReturnArg::Handle,
		Some(&[protocol]),
		srv_id
	)
}

pub fn open(endpoint: &str, protocols: &[u128], ctor_args: User<*const u8>) -> Result<Arc<Handle>, Error> {
	let (srv_id, srv, endpoint) = open_server(endpoint)?;

	Ok(match srv.ctor(endpoint, CtorArgs::new(&*protocols, ctor_args))? {
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

pub fn init_ramdisk(data: Box<[u8]>) -> ServerId {
	server::server_registry_mut().insert_server(
		None,
		ServerTy::Ramdisk(server::ramdisk::RamdiskServer::new(data)),
	).expect("failed to start ramdisk server")
}
