use crate::prelude::*;
use hashbrown::HashMap;
use kernel_api::sync::{LazyLock, RwSpinlock};
use crate::{hashmap_new, non_zero};
use crate::ipc::Error;
use crate::ipc::server::MethodResult;

mod std_shim;

pub static PROTOCOL_REGISTRY: LazyLock<RwSpinlock<HashMap<u128, meta::Meta>>> = LazyLock::new(|| {
	// fixme: generate pipb files in build.rs, include them here, then run the generic parser
	let mut map = hashmap_new!();

	// core.proc.Thread
	map.extend([
		// unstable_anon_alloc
		(
			<dyn generated::CoreProcThread>::UID | 1 << 96,
			meta::Meta::Method([meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None], meta::ReturnArg::Primitive)
		),
		// unstable_anon_dealloc
		(
			<dyn generated::CoreProcThread>::UID | 2 << 96,
			meta::Meta::Method([meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None], meta::ReturnArg::None)
		),
		// set_tcb
		(
			<dyn generated::CoreProcThread>::UID | 3 << 96,
			meta::Meta::Method([meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None], meta::ReturnArg::None)
		),
		// spawn_thread
		(
			<dyn generated::CoreProcThread>::UID | 4 << 96,
			meta::Meta::Method(
				[meta::Arg::MemoryInPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::Primitive, meta::Arg::Primitive],
				meta::ReturnArg::Handle,
			)
		),
		// yield_now
		(
			<dyn generated::CoreProcThread>::UID | 5 << 96,
			meta::Meta::Method([meta::Arg::None, meta::Arg::None, meta::Arg::None, meta::Arg::None], meta::ReturnArg::None)
		),
	].into_iter());

	// core.io.Read
	map.extend([
		// read
		(
			<dyn generated::CoreIoRead>::UID | 1 << 96,
			meta::Meta::Method([meta::Arg::MemoryOutPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::None, meta::Arg::None], meta::ReturnArg::Primitive)
		),
	].into_iter());

	// core.io.Write
	map.extend([
		// write
		(
			<dyn generated::CoreIoWrite>::UID | 1 << 96,
			meta::Meta::Method([meta::Arg::MemoryInPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::None, meta::Arg::None], meta::ReturnArg::Primitive)
		),
	].into_iter());

	// core.io.Seek - nothing
	// core.fs.File - nothing

	// core.server.Sync
	map.extend([
		// next
		(
			<dyn generated::CoreServerSync>::UID | 1 << 96,
			meta::Meta::Method([meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None], meta::ReturnArg::None)
		),
		// reply
		(
			<dyn generated::CoreServerSync>::UID | 2 << 96,
			meta::Meta::Method([meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None], meta::ReturnArg::None)
		),
		// forge
		(
			<dyn generated::CoreServerSync>::UID | 3 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::MemoryInPtr { len_arg: non_zero!(3) }, meta::Arg::Primitive, meta::Arg::None],
				meta::ReturnArg::Handle,
			)
		),
	].into_iter());

	// core.proc.Builder
	map.extend([
		// spawn
		(
			<dyn generated::CoreProcBuilder>::UID | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::None, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Handle,
			)
		),
		// add_handle
		(
			<dyn generated::CoreProcBuilder>::UID | 2 << 96,
			meta::Meta::Method([meta::Arg::MemoryInPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::Handle, meta::Arg::None], meta::ReturnArg::None)
		),
	].into_iter());

	RwSpinlock::new(map)
});

pub mod meta {
	use core::num::NonZero;

	#[derive(Debug, Clone)]
	pub enum Meta {
		Ctor(),
		Method([Arg; 4], ReturnArg),
	}

	#[derive(Debug, Copy, Clone)]
	pub enum Arg {
		Primitive,
		MemoryInPtr { len_arg: NonZero<u8> },
		MemoryOutPtr { len_arg: NonZero<u8> },
		Handle,
		None,
	}

	#[derive(Debug, Copy, Clone, Eq, PartialEq)]
	pub enum ReturnArg {
		Primitive,
		Handle,
		None,
	}
}

pub struct DispatchTable {
	map: HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<MethodResult, Error>>,
}

impl DispatchTable {
	pub fn new() -> Self { Self { map: HashMap::new() } }
	pub fn add_vtable(mut self, vtable: HashMap<u128, fn(*const (), usize, usize, usize, usize, usize) -> Result<MethodResult, Error>>) -> Self {
		self.map.extend(vtable);
		self
	}

	pub fn dispatch(
		&self,
		protocol: u128,
		method: u32,
		f_self: *const (),
		arg0: usize,
		arg1: usize,
		arg2: usize,
		arg3: usize,
		arg4: usize
	) -> Result<MethodResult, Error> {
		let uid = protocol | (method as u128) << 96;
		let f = self.map.get(&uid).ok_or(Error::UnsupportedProtocol)?;
		f(f_self, arg0, arg1, arg2, arg3, arg4)
	}
	
	/*pub fn ctor_deserialize(&self, protocol: u128) -> Result<fn(buffer: &mut User<*const u8>) -> Result<Box<[u8]>, Error>, Error> {
		let f = *self.map.get(&protocol).ok_or(Error::UnsupportedProtocol)?;
		let f = unsafe { core::mem::transmute(f) };
		Ok(f)
	}*/
}

pub trait Protocol {
	const UID: u128;
	type Ctor;
}

pub mod generated {
	#![allow(unused)]
	
	use super::std_shim as std;
	
	trait StrExt {
		fn new(&self) -> &Self;
	}
	
	impl StrExt for str {
		fn new(&self) -> &Self {
			self
		}
	}

	use crate::prelude::*;

	include!(concat!(env!("OUT_DIR"), "/protocol.gen.rs"));
}