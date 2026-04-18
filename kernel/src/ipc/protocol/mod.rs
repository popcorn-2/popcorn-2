use core::future::Future;
use core::pin::Pin;
use hashbrown::HashMap;
use kernel_api::sync::{LazyLock, RwSpinlock};
use kernel_api::syscall;
use kernel_api::syscall::Error;
use crate::{hashmap_new, non_zero};
use crate::ipc::serde::MethodResult;

mod std_shim;

pub static PROTOCOL_REGISTRY: LazyLock<RwSpinlock<HashMap<u128, meta::Meta>>> = LazyLock::new(|| {
	// fixme: generate pipb files in build.rs, include them here, then run the generic parser
	let mut map = hashmap_new!();

	// core.mem.Pager
	map.extend([
		// get_pages
		(
			<dyn generated::core::mem::Pager>::UID | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::Primitive, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Primitive,
				"get_pages@core.mem.Pager",
			)
		)
	]);

	// core.proc.Thread
	map.extend([
		// unstable_anon_alloc
		(
			<dyn generated::core::proc::Thread>::UID | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Primitive,
				"unstable_anon_alloc@core.proc.Thread",
			),
		),
		// unstable_anon_dealloc
		(
			<dyn generated::core::proc::Thread>::UID | 2 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::None,
				"unstable_anon_dealloc@core.proc.Thread",
			),
		),
		// set_tcb
		(
			<dyn generated::core::proc::Thread>::UID | 3 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::None,
				"set_tcb@core.proc.Thread",
			),
		),
		// spawn_thread
		(
			<dyn generated::core::proc::Thread>::UID | 4 << 96,
			meta::Meta::Method(
				[meta::Arg::MemoryInPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::Primitive, meta::Arg::Primitive],
				meta::ReturnArg::Handle,
				"spawn_thread@core.proc.Thread",
			),
		),
		// yield_now
		(
			<dyn generated::core::proc::Thread>::UID | 5 << 96,
			meta::Meta::Method(
				[meta::Arg::None, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::None,
				"yield_now@core.proc.Thread",
			),
		),
		// unstable_mmio_alloc
		(
			<dyn generated::core::proc::Thread>::UID | 6 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::Primitive, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Primitive,
				"unstable_mmio_alloc@core.proc.Thread",
			)
		),
		// map_vmo
		(
			<dyn generated::core::proc::Thread>::UID | 9 << 96,
			meta::Meta::Method(
				[meta::Arg::Handle, meta::Arg::Primitive, meta::Arg::Primitive, meta::Arg::Primitive],
				meta::ReturnArg::Primitive,
				"map_vmo@core.proc.Thread",
			),
		),
	].into_iter());

	// core.io.Read
	map.extend([
		// read
		(
			<dyn generated::core::io::Read>::UID | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::MemoryOutPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Primitive,
				"read@core.io.Read",
			)
		),
	].into_iter());

	// core.io.Write
	map.extend([
		// write
		(
			<dyn generated::core::io::Write>::UID | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::MemoryInPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Primitive,
				"write@core.io.Write",
			)
		),
	].into_iter());

	// core.io.Seek
	map.extend([
		// tell
		(
			<dyn generated::core::io::Seek>::UID | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::None, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Primitive,
				"tell@core.io.Seek",
			)
		),
		// seek
		(
			<dyn generated::core::io::Seek>::UID | 2 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::None,
				"seek@core.io.Seek",
			)
		),
	].into_iter());
	// core.fs.File - nothing

	// core.server.Sync
	map.extend([
		// next
		(
			<dyn generated::core::server::Sync>::UID | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::None,
				"next@core.server.Sync",
			)
		),
		// reply
		(
			<dyn generated::core::server::Sync>::UID | 2 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::None,
				"reply@core.server.Sync",
			)
		),
		// forge
		(
			<dyn generated::core::server::Sync>::UID | 3 << 96,
			meta::Meta::Method(
				[meta::Arg::Primitive, meta::Arg::MemoryInPtr { len_arg: non_zero!(3) }, meta::Arg::Primitive, meta::Arg::None],
				meta::ReturnArg::Handle,
				"forge@core.server.Sync",
			)
		),
	].into_iter());

	// core.proc.Builder
	map.extend([
		// spawn
		(
			<dyn generated::core::proc::Builder>::UID | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::None, meta::Arg::None, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Handle,
				"spawn@core.proc.Builder",
			)
		),
		// add_handle
		(
			<dyn generated::core::proc::Builder>::UID | 2 << 96,
			meta::Meta::Method(
				[meta::Arg::MemoryInPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::Handle, meta::Arg::None],
				meta::ReturnArg::None,
				"add_handle@core.proc.Builder",
			)
		),
	].into_iter());

	// driver.BusNode
	map.extend([
		// create_child
		(
			0x1001 | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::MemoryInPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::None, meta::Arg::None],
				meta::ReturnArg::Handle,
				"create_child@driver.BusNode",
			),
		),
	]);

	// driver.Acpi
	map.extend([
		// read_table
		(
			0x1002 | 1 << 96,
			meta::Meta::Method(
				[meta::Arg::MemoryOutPtr { len_arg: non_zero!(2) }, meta::Arg::Primitive, meta::Arg::MemoryInPtr { len_arg: non_zero!(4) }, meta::Arg::Primitive],
				meta::ReturnArg::Primitive,
				"read_table@driver.Acpi",
			)
		),
	].into_iter());

	RwSpinlock::new(map)
});

#[cfg_attr(not(debug_assertions), expect(unused))]
pub fn syscall_name(protocol: u128, method: u32) -> String {
	if cfg!(debug_assertions) {
		let registry = PROTOCOL_REGISTRY.read();
		let uid = protocol | (method as u128) << 96;
		if let Some(meta::Meta::Method(_, _, name)) = registry.get(&uid) {
			return format!("{}", name);
		}
	}

	format!("{:#x}@{:024x}", method, protocol)
}

pub mod meta {
	use core::num::NonZero;

	#[derive(Debug, Clone)]
	pub enum Meta {
		Method([Arg; 4], ReturnArg, &'static str),
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
	map: HashMap<u128, for<'a> fn(&'a (), usize, usize, usize, usize, usize) -> Pin<Box<dyn Send + 'a + Future<Output = Result<MethodResult, Error>>>>>,
}

impl DispatchTable {
	pub fn new() -> Self { Self { map: HashMap::new() } }
	pub fn add_vtable(mut self, vtable: HashMap<u128, for<'a> fn(&'a (), usize, usize, usize, usize, usize) -> Pin<Box<dyn Send + 'a + Future<Output = Result<MethodResult, Error>>>>>) -> Self {
		self.map.extend(vtable);
		self
	}

	pub fn dispatch<'a>(
		&self,
		protocol: u128,
		method: u32,
		f_self: &'a (),
		arg0: usize,
		arg1: usize,
		arg2: usize,
		arg3: usize,
		arg4: usize,
	) -> syscall::Result<Pin<Box<dyn Send + 'a + Future<Output = syscall::Result<MethodResult>>>>> {
		let uid = protocol | (method as u128) << 96;
		let f = self.map.get(&uid).ok_or(Error::UnsupportedProtocol)?;
		Ok(f(f_self, arg0, arg1, arg2, arg3, arg4))
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

	mod popcorn_server {
		pub use crate::ipc::serde::MethodResult as Result;
		pub use crate::ipc::server::ReturnHandle as ReturnHandle;
	}
	
	trait StrExt {
		fn new(&self) -> &Self;
	}
	
	impl StrExt for str {
		fn new(&self) -> &Self {
			self
		}
	}


	include!(concat!(env!("OUT_DIR"), "/protocol.gen.rs"));
}
