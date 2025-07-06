use alloc::sync::Arc;
use alloc::vec;
use itertools::Itertools;
use crate::ipc::handle::{Handle, ServerId};
use kernel_api::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, User};
use crate::ipc::protocol::meta::{self, Meta};
use crate::ipc::server::MethodResult;
use crate::memory::r#virtual::AddressSpaceInner;
use super::{Error, protocol};
use crate::prelude::*;

#[derive(Clone, Debug)]
pub enum Arg {
	Primitive(usize),
	BufferOffset(usize),
	NewBuffer,
	Handle(Arc<Handle>),
}

#[derive(Debug)]
pub enum Return {
	Boxed(Box<[u8]>),
	Value(u128),
	Handle(Arc<Handle>),
	NewHandle(isize, Box<[u128]>),
	NewDefaultHandle(isize),
}

impl From<MethodResult> for Return {
	fn from(value: MethodResult) -> Self {
		match value {
			MethodResult::SelfHandle(id, protos) => Return::NewHandle(id, protos),
			MethodResult::SelfDefaultHandle(id) => Return::NewDefaultHandle(id),
			MethodResult::TransferHandle(handle) => Return::Handle(handle),
			MethodResult::Value(val) => Return::Value(val),
		}
	}
}

#[derive(Debug)]
pub struct DeserializedArgs {
	pub buffer: Box<[u8]>,
	pub args: [Arg; 5],
	pub return_size: Option<usize>,
	pub return_ptr: User<*mut [u8]>,
	pub return_ty: meta::ReturnArg,
}

pub fn deserialize(uid: u128, args: &[usize; 5]) -> Result<DeserializedArgs, Error> {
	let (meta, return_ty) = {
		let meta = protocol::PROTOCOL_REGISTRY.read().get(&uid)
				                               .cloned()
		                                       .ok_or(Error::UnknownProtocol)?;
		let Meta::Method(meta, return_ty) = meta else { return Err(Error::Invalid); };
		(meta, return_ty)
	};

	let forward_args = core::iter::once(Arg::Primitive(args[0]))
			.chain(core::iter::repeat(Arg::Primitive(0)))
			.next_array::<5>()
			.expect("iterator has infinite items");
	
	let mut buffer = vec![];
	let mut deserialized = DeserializedArgs {
		buffer: Box::from([]),
		args: forward_args,
		return_size: None,
		return_ptr: slice_from_raw_parts_mut(User::null_mut(), 0),
		return_ty,
	};

	for i in 1..5 { // skip handle arg
		trace!("arg {i} = {:#x} ({:?})", args[i], meta[i - 1]);
		deserialized.args[i] = match meta[i - 1] {
			meta::Arg::Primitive => Arg::Primitive(args[i]),
			meta::Arg::None => continue,
			meta::Arg::Handle => {
				// this is guaranteed to always run in the calling thread context, so we can use the
				// current thread handle map
				let handle = args[i].try_into().map_err(|_| Error::InvalidHandle)?;
				let handle = percpu_v2!(current_thread).read().as_ref()
				                                       .expect("must be running on a thread to syscall")
				                                       .tcb_ref().handles.pop(handle)?; // any move of a handle into a syscall is destructive
				Arg::Handle(handle)
			}
			meta::Arg::MemoryInPtr { len_arg } => {
				// this is guaranteed to always run in the calling thread context, so we can use the
				// current address space
				let len = args[len_arg.get() as usize];
				let data = {
					let ptr = args[i];
					let ptr = User::<*const u8>::new_in(
						ptr as *const u8,
						AddressSpaceInner::to_api(
							percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
							                          .tcb_ref().address_space
						),
					);
					slice_from_raw_parts(ptr, len)
				};
				debug!("data: {data:#p}");
				let data = unsafe { data.read_to_buffer() }?;
				debug!("data: {data:#x?}");

				let arg = Arg::BufferOffset(buffer.len());
				buffer.extend(data);
				arg
			},
			meta::Arg::MemoryOutPtr { len_arg } => {
				let len = args[len_arg.get() as usize];
				debug_assert!(deserialized.return_size.is_none(), "cannot have more than one return arg");
				deserialized.return_size = Some(len);
				deserialized.return_ptr = {
					let ptr = User::<*mut u8>::new_in(
						args[i] as *mut u8,
						AddressSpaceInner::to_api(
							percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
							                          .tcb_ref().address_space
						),
					);
					slice_from_raw_parts_mut(ptr, len)
				};
				Arg::NewBuffer
			}
		}
	}
	
	deserialized.buffer = buffer.into_boxed_slice();

	Ok(deserialized)
}

pub fn serialize(
	value: Return,
	return_ptr: User<*mut [u8]>,
	expected_ret: meta::ReturnArg,
	default_protocol: Option<&[u128]>,
	server_id: ServerId,
) -> Result<u128, Error> {
	Ok(match value {
		Return::Boxed(boxed) => {
			return_ptr.write_from_buffer(&boxed)? as u128
		}
		Return::Value(val) => val as u128,
		Return::Handle(handle) => {
			let res = percpu_v2!(current_thread).read().as_ref()
			                                    .expect("must be running on a thread to syscall")
			                                    .tcb_ref().handles.push(handle);
			res.map(|val| val as u128)?
		}
		Return::NewHandle(handle, protos) => {
			let handle = Handle::new(
				server_id,
				handle,
				&*protos
			);
			let res = percpu_v2!(current_thread).read().as_ref()
			                                    .expect("must be running on a thread to syscall")
			                                    .tcb_ref().handles.push(handle);
			res.map(|val| val as u128)?
		}
		Return::NewDefaultHandle(handle) => {
			let Some(default_protocol) = default_protocol else { unreachable!("server returned default protocol for non-ctor"); };
			let handle = Handle::new(
				server_id,
				handle,
				default_protocol,
			);
			let res = percpu_v2!(current_thread).read().as_ref()
			                                    .expect("must be running on a thread to syscall")
			                                    .tcb_ref().handles.push(handle);
			res.map(|val| val as u128)?
		}
	})
}
