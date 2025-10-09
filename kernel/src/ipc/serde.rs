use alloc::sync::Arc;
use core::cmp::min;
use core::fmt::{Debug, Formatter};
use core::mem::MaybeUninit;
use core::ops::Range;
use core::pin::Pin;
use kernel_api::dbg;
use kernel_api::memory::VirtualAddress;
use kernel_api::ptr::{local_slice_from_raw_parts, local_slice_from_raw_parts_mut, LocalUser};
use kernel_api::sync::SendWrapper;
use kernel_api::syscall::handle::Handle;
use kernel_api::syscall::server::ServerId;
use crate::ipc::{Error, protocol};
use crate::ipc::protocol::meta::{Arg, Meta};
use crate::ipc::server::ReturnHandle;

pub fn deserialize(protocol: u128, method: u32, handle: &Arc<Handle>, args: &[usize; 4]) -> Result<Deserialized, Error> {
	let meta = {
		let meta = protocol::PROTOCOL_REGISTRY.read().get(&((method as u128) << 96 | protocol))
		                                      .cloned()
		                                      .ok_or(Error::UnknownProtocol)?;
		let Meta::Method(meta, _return_ty, _) = meta; // else { return Err(Error::Invalid); };
		meta
	};

	let mut forward_args = [const { InputArg::RawValue(0) }; 5];

	let (server_id, handle_id) = match handle.id(protocol) {
		Ok(val) => val,
		Err(e) => {
			debug!("handle only supports {handle:#x?}");
			return Err(e);
		},
	};
	forward_args[0] = InputArg::RawValue(handle_id as usize);

	let mut buffer = Vec::new();
	let mut return_buffer = None;

	for i in 1..5 {
		forward_args[i] = match meta[i - 1] {
			Arg::Primitive => InputArg::RawValue(args[i - 1]),
			Arg::MemoryInPtr { len_arg } => {
				let len = args[(len_arg.get() as usize) - 1];
				let ptr = {
					let ptr = unsafe { LocalUser::<*const u8>::new(args[i - 1]) };
					unsafe { local_slice_from_raw_parts(ptr, len) }
				};
				let buf_offset = buffer.len();
				buffer.reserve(len);
				let written = ptr.read_to_buffer(buffer.spare_capacity_mut())?;
				unsafe { buffer.set_len(buf_offset + written) };
				InputArg::BufferOffset(buf_offset)
			},
			Arg::MemoryOutPtr { len_arg } => {
				let len = args[(len_arg.get() as usize) - 1];
				let buf_offset = buffer.len();
				buffer.extend(core::iter::repeat_n(0, len));
				let local_ptr = SendWrapper::new(LocalUser::<*mut MaybeUninit<u8>>::new(args[i - 1]));
				return_buffer = Some((
					buf_offset .. (buf_offset + len),
					local_ptr,
				));
				InputArg::BufferOffset(buf_offset)
			},
			Arg::Handle => InputArg::Handle(get_handle(args[i - 1])?),
			Arg::None => InputArg::RawValue(0),
		};
	}
	
	let buffer = unsafe { Box::from_raw(Box::into_raw(buffer.into_boxed_slice()) as *mut [MaybeUninit<u8>]) };

	Ok(
		Deserialized {
			server: server_id,
			buffer: Pin::new(buffer),
			args: forward_args,
			return_buffer,
		}
	)
}

pub fn get_handle(arg: usize) -> Result<Arc<Handle>, Error> {
	let handle = arg.try_into().map_err(|_| Error::InvalidHandle)?;
	let handle = percpu_v2!(current_thread)
			.read()
			.as_ref()
			.expect("must be running on thread to syscall")
			.handles
			.pop(handle)?;
	Ok(handle)
}

pub struct Deserialized {
	server: ServerId,
	buffer: Pin<Box<[MaybeUninit<u8>]>>,
	args: [InputArg; 5],
	return_buffer: Option<(Range<usize>, SendWrapper<LocalUser<*mut MaybeUninit<u8>>>)>,
}

impl Debug for Deserialized {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Deserialized")
			.field("server", &self.server)
			.field("args", &self.args)
			.field("return_buffer", &self.return_buffer)
			.finish_non_exhaustive()
	}
}

pub struct Serializer {
	server: ServerId,
	buffer: Pin<Box<[MaybeUninit<u8>]>>,
	default_protos: Box<[u128]>,
	return_buffer: Option<(Range<usize>, SendWrapper<LocalUser<*mut MaybeUninit<u8>>>)>,
}

impl Debug for Serializer {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Serializer")
		 .field("server", &self.server)
		 .field("default_protos", &self.default_protos)
		 .field("return_buffer", &self.return_buffer)
		 .finish_non_exhaustive()
	}
}

impl Deserialized {
	pub fn server(&self) -> ServerId { self.server }

	pub fn buffer(&self) -> Pin<&[MaybeUninit<u8>]> {
		self.buffer.as_ref()
	}

	pub fn into_args_with_buffer(self, buffer_base: VirtualAddress) -> ([usize; 5], Serializer) {
		let args = self.args.map(|arg| match arg {
			InputArg::RawValue(val) => val,
			InputArg::BufferOffset(offset) => buffer_base.addr + offset,
			InputArg::Handle(handle) => Arc::into_raw(handle).expose_provenance(),
		});

		(
			args,
			Serializer {
				server: self.server,
				buffer: self.buffer,
				default_protos: Box::from([]),
				return_buffer: self.return_buffer,
			}
		)
	}

	pub fn into_args(self) -> ([usize; 5], Serializer) {
		let base = self.buffer.as_ptr().into();
		self.into_args_with_buffer(base)
	}

	pub fn new_buffer_input(
		server: ServerId,
		args: [InputArg; 5],
		buffer: Box<[u8]>,
	) -> Self {
		let buffer_len = buffer.len();
		let buffer_ptr = Box::into_raw(buffer)
				.as_mut_ptr();
		let buffer_ptr = core::ptr::from_raw_parts_mut::<[MaybeUninit<u8>]>(
			buffer_ptr,
			buffer_len
		);
		let buffer = Pin::new(
			unsafe { Box::<[MaybeUninit<u8>]>::from_raw(buffer_ptr) }
		);
		Self {
			server,
			buffer,
			args,
			return_buffer: None,
		}
	}
}

#[derive(Clone, Debug)]
pub enum InputArg {
	RawValue(usize),
	BufferOffset(usize),
	Handle(Arc<Handle>),
}

impl Serializer {
	pub fn serialize(self, result: Result<MethodResult, Error>) -> Result<u128, Error> {
		let result = result?;
		if let Some(return_buffer) = self.return_buffer {
			let MethodResult::Value(count) = result else { return Err(Error::InvalidReturn); };
			let count = min(return_buffer.0.len(), count as usize);
			let return_buffer_range = return_buffer.0.start .. (return_buffer.0.start + count);

			let ptr = {
				let ptr = return_buffer.1.into_inner();
				local_slice_from_raw_parts_mut(ptr, count)
			};

			let count = ptr.write_from_slice(&self.buffer[return_buffer_range])?;

			Ok(count as u128)
		} else {
			match result {
				MethodResult::SelfHandle(id, protos) => {
					let handle = Handle::new(self.server, id, &protos);
					percpu_v2!(current_thread)
							.read()
					        .as_ref()
							.expect("cannot syscall from idle")
							.handles
							.push(dbg!(handle))
							.map(|v| v as u128)
				}
				MethodResult::SelfDefaultHandle(id) => {
					let handle = Handle::new(self.server, id, &self.default_protos);
					percpu_v2!(current_thread)
							.read()
							.as_ref()
							.expect("cannot syscall from idle")
							.handles
							.push(dbg!(handle))
							.map(|v| v as u128)
				}
				MethodResult::TransferHandle(handle) => {
					percpu_v2!(current_thread)
							.read()
							.as_ref()
							.expect("cannot syscall from idle")
							.handles
							.push(dbg!(handle))
							.map(|v| v as u128)
				}
				MethodResult::Value(val) => Ok(val),
			}
		}
	}

	pub fn buffer_uninit(&mut self) -> Pin<&mut [MaybeUninit<u8>]> {
		self.buffer.as_mut()
	}
	
	pub fn new(server: ServerId, default_protos: Box<[u128]>) -> Self {
		Self {
			server,
			buffer: Pin::new(Box::from([])),
			default_protos,
			return_buffer: None,
		}
	}

	pub fn set_default_protos(&mut self, protos: Box<[u128]>) {
		self.default_protos = protos;
	}
}

#[derive(Debug)]
pub enum MethodResult {
	SelfHandle(isize, Box<[u128]>),
	SelfDefaultHandle(isize),
	TransferHandle(Arc<Handle>),
	Value(u128),
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