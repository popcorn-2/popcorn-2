use alloc::sync::Arc;
use core::str::pattern::{Pattern, Searcher};
use hashbrown::HashMap;
use crate::prelude::*;
use crate::memory::r#virtual::AddressSpaceInner;
use kernel_api::ptr::{PointerError, slice_from_raw_parts, User};
use kernel_api::sync::OnceLock;
use crate::ipc::ctor::CtorArgs;
use crate::ipc::handle::Handle;

pub mod handle;
pub mod protocol;
mod ctor;
pub mod server;

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
}

impl From<PointerError> for Error {
	fn from(_: PointerError) -> Self {
		Self::InvalidPointer
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
	if protocol == 0 && method == 1 {
		// new@abi.v1 - special cased

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

		let Ok(endpoint) = (unsafe { endpoint.read_to_buffer() }) else { yeet!(Error::InvalidPointer); };

		let endpoint = match ::core::str::from_utf8(&endpoint) {
			Ok(endpoint) => endpoint,
			Err(e) => {
				error!("{e:?}");
				yeet!(Error::InvalidUtf8);
			},
		};

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

		let Ok(protocols) = (unsafe { protocols.read_to_buffer() }) else { yeet!(Error::InvalidPointer); };

		let ctor_args_ptr = User::<*const u8>::new_in(
			arg4 as _,
			AddressSpaceInner::to_api(
				percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
				                          .tcb_ref().address_space
			),
		);

		let handle = open(endpoint, protocols, ctor_args_ptr)?;

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
					arg2 as _,
					AddressSpaceInner::to_api(
						percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
						                          .tcb_ref().address_space
					),
				);
				slice_from_raw_parts(protocol_ptr, arg3)
			};
			let protocols = unsafe { protocols.read_to_buffer() }?;
			
			return Ok(handle.has_protocols(&*protocols) as u128);
		}
		
		let srv = server::server_registry().get_server(handle.server_id())?;
		let dispatch_table = srv.dispatch_table();
		dispatch_table.dispatch(
			protocol,
			method,
			srv.this(),
			handle.internal_id() as usize,
			arg1, arg2, arg3, arg4
		)
	}
}

pub fn open(endpoint: &str, protocols: Box<[u128]>, args_ptr: User<*const u8>) -> Result<Handle, Error> {
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

	let (srv_id, srv) = server::server_registry().get_server_at(domain)?;
	debug!("Open `{srv_id:?}:{endpoint}`");
	
	Ok(Handle::new(
		srv_id,
		srv.ctor(endpoint, CtorArgs::new(&*protocols, args_ptr))?,
		&*protocols
	))
}
