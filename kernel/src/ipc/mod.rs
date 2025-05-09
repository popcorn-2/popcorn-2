#[allow(unused_imports)] use crate::prelude::*;

pub mod server;
pub mod handle;
mod protocol;

use utils::better_cow::Cow;
use kernel_api::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};
use ::core::str::pattern::{Pattern, Searcher};
use hashbrown::HashMap;
use kernel_api::ptr::User;
use kernel_api::sync::{LazyLock, RwSpinlock};
use crate::ipc::handle::Handle;
use crate::ipc::protocol::{ArgPairTy, ArgTy, HandledMethod, Method};
use server::Server as _;
use crate::memory::r#virtual::AddressSpaceInner;

mod core_protos {
	pub mod server {
		pub const SYNC: u128 = 0x7;
		
		pub const SYNC_GET: u128 = 0;
		pub const SYNC_POST: u128 = 1<<96;
	}
	pub mod io {
		pub const WRITE: u128 = 0x2;
		pub const READ: u128 = 0x3;
		pub const SEEK: u128 = 0x4;

		pub const WRITE_WRITE: u128 = 0;
		pub const READ_READ: u128 = 0;
		pub const SEEK_SEEK: u128 = 0;
	}
	pub mod proc {
		pub const PROC: u128 = 0x5;
		pub const THREAD: u128 = 0x6;

		pub const PROC_EXIT: u128 = 0<<96;
		pub const PROC_DEBUG: u128 = 1<<96;
		pub const PROC_ALLOC: u128 = 2<<96;
		pub const PROC_DEALLOC: u128 = 3<<96;
		pub const THREAD_SET_TCB: u128 = 0<<96;
		pub const THREAD_EXEC: u128 = 1<<96;
		pub const THREAD_JOIN: u128 = 2<<96;
	}
}

static METHODS: LazyLock<HashMap<u128, Method>> = LazyLock::new(|| {
	let mut map = HashMap::new();

	map.try_insert(
		core_protos::io::WRITE | core_protos::io::WRITE_WRITE,
		HandledMethod {
			a: ArgTy::None,
			b: ArgPairTy::String, // todo
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::io::READ | core_protos::io::READ_READ,
		HandledMethod {
			a: ArgTy::None,
			b: ArgPairTy::OutMemory,
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::proc::PROC | core_protos::proc::PROC_DEBUG,
		HandledMethod {
			a: ArgTy::None,
			b: ArgPairTy::String,
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::proc::PROC | core_protos::proc::PROC_EXIT,
		HandledMethod {
			a: ArgTy::Value,
			b: ArgPairTy::None,
			ret: ArgTy::None,
		}
	).unwrap();
	map.try_insert(
		core_protos::proc::PROC | core_protos::proc::PROC_ALLOC,
		HandledMethod {
			a: ArgTy::Value,
			b: ArgPairTy::None,
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::proc::PROC | core_protos::proc::PROC_DEALLOC,
		HandledMethod {
			a: ArgTy::Value,
			b: ArgPairTy::None,
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::proc::THREAD | core_protos::proc::THREAD_SET_TCB,
		HandledMethod {
			a: ArgTy::Value,
			b: ArgPairTy::None,
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::proc::THREAD | core_protos::proc::THREAD_EXEC,
		HandledMethod {
			a: ArgTy::Value,
			b: ArgPairTy::Pair(ArgTy::Value, ArgTy::Value),
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::proc::THREAD | core_protos::proc::THREAD_JOIN,
		HandledMethod {
			a: ArgTy::None,
			b: ArgPairTy::None,
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::server::SYNC | core_protos::server::SYNC_GET,
		HandledMethod {
			a: ArgTy::None,
			b: ArgPairTy::OutMemory,
			ret: ArgTy::Value,
		}
	).unwrap();
	map.try_insert(
		core_protos::server::SYNC | core_protos::server::SYNC_POST,
		HandledMethod {
			a: ArgTy::None,
			b: ArgPairTy::Memory,
			ret: ArgTy::Value,
		}
	).unwrap();

	debug!("{map:#x?}");

	map
});

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i16)]
pub enum Error {
	Unimplemented = 1,
	InvalidUtf8 = 2,
	Overflow = 3,
	InvalidPointer = 4,
	InvalidArg = 5,
	NameInUse = 6,
	BadServer = 7,
	BadHandle = 8,
	ServerDead = 9,
	AllocationFailure = 10,
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
// Invariants: minimum value of 0
pub struct NonNegativeIsize(isize);

impl NonNegativeIsize {
	pub const fn new(n: isize) -> Option<Self> {
		match n {
			0.. => Some(Self(n)),
			_ => None
		}
	}

	pub const fn get(self) -> isize {
		debug_assert!(self.0 >= 0, "NonNegativeIsize has negative value" /* FIXME(const `Display`): "NonNegativeIsize has negative value of `{}`", self.0 */);
		self.0
	}
}

pub extern "C" fn syscall_entry(proto_method: u128, a: usize, b: usize, c: usize, d: usize) -> isize {
	let res = syscall(proto_method, a, b, c, d);
	match res {
		Ok(val) => val.get(),
		Err(e) => {
			let err = -(e as i16);
			debug_assert!(err < 0, "Error condition should be negative");
			isize::from(err)
		}
	}
}

fn syscall(proto_method: u128, a: usize, b: usize, c: usize, d: usize) -> Result<NonNegativeIsize, Error> {
	if proto_method == 0 /* open@core.object.Object */ {
		// For now, we special case `open` as the only static function (not taking a `Handle`, and
		// thus requiring extra knowledge by the kernel to know where to dispatch it)

		let endpoint_ptr = {
			let endpoint_ptr = User::<*const u8>::new_in(
				a as _,
				AddressSpaceInner::to_api(
					percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
							.tcb_ref().address_space
				),
			);
			slice_from_raw_parts(endpoint_ptr, b)
		};
		let proto_ptr = {
			let proto_ptr = User::<*const u128>::new_in(
				c as _,
				AddressSpaceInner::to_api(
					percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
					                          .tcb_ref().address_space
				),
			);
			slice_from_raw_parts(proto_ptr, d)
		};

		let Ok(buf) = (unsafe { endpoint_ptr.read_to_buffer() }) else { yeet!(Error::InvalidPointer); };

		let path = match ::core::str::from_utf8(&buf) {
			Ok(path) => path,
			Err(e) => {
				error!("{buf:#x?}\n{e:?}");
				yeet!(Error::InvalidUtf8);
			},
		};
		
		open(path).and_then(|handle| {
			let res = percpu_v2!(current_thread).read().as_ref()
					.expect("must be running on a thread to syscall")
					.tcb_ref().handles.push(handle);
			res.map(|val| NonNegativeIsize::new(val.try_into().unwrap()).unwrap())
		})
	} else {
		let Some(meta) = METHODS.get(&proto_method).map(|&x| x) else {
			yeet!(Error::Unimplemented);
		};

		let handle = percpu_v2!(current_thread).read().as_ref()
		                                       .expect("must be running on a thread to syscall")
		                                       .tcb_ref().handles.get(a.try_into().unwrap())?;

		debug!("dispatch to {handle:x?}");
		
		let servers = server::servers();
		let srv = servers.get_server(handle.server_id())?;
		
		match (meta.a, meta.b, meta.ret) {
			(ArgTy::Value | ArgTy::None, ArgPairTy::Pair(ArgTy::Value | ArgTy::None, ArgTy::Value | ArgTy::None) | ArgPairTy::None, ArgTy::Value | ArgTy::None) => {
				srv.dispatch_vvv_ve(
					proto_method,
					handle.internal_id(),
					b, c, d
				)
			}
			(ArgTy::Value | ArgTy::None, ArgPairTy::String, ArgTy::Value | ArgTy::None) => {
				let s = {
					let arg_ptr = User::<*const u8>::new_in(
						c as _,
						AddressSpaceInner::to_api(
							percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
							                          .tcb_ref().address_space
						),
					);
					let arg_ptr = slice_from_raw_parts(arg_ptr, d);

					let Ok(buf) = (unsafe { arg_ptr.read_to_buffer() }) else { yeet!(Error::InvalidPointer); };

					match String::from_utf8(buf.to_vec()) {
						Ok(path) => path,
						Err(e) => {
							error!("{buf:#x?}\n{e:?}");
							yeet!(Error::InvalidUtf8);
						},
					}
				};

				srv.dispatch_vs_ve(
					proto_method,
					handle.internal_id(),
					b, s
				)
			}
			(ArgTy::Value | ArgTy::None, ArgPairTy::Memory, ArgTy::Value | ArgTy::None) => {
				let m = {
					let arg_ptr = User::<*const u8>::new_in(
						c as _,
						AddressSpaceInner::to_api(
							percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
							                          .tcb_ref().address_space
						),
					);
					let arg_ptr = slice_from_raw_parts(arg_ptr, d);

					let Ok(buf) = (unsafe { arg_ptr.read_to_buffer() }) else { yeet!(Error::InvalidPointer); };

					buf
				};
				srv.dispatch_vm_ve(
					proto_method,
					handle.internal_id(),
					b, m
				)
			}
			(ArgTy::Value | ArgTy::None, ArgPairTy::OutMemory, ArgTy::Value | ArgTy::None) => {
				let (out, res) = srv.dispatch_vM_ve(
					proto_method,
					handle.internal_id(),
					b, d
				)?;

				let arg_ptr = User::<*mut u8>::new_in(
					c as _,
					AddressSpaceInner::to_api(
						percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
						                          .tcb_ref().address_space
					),
				);
				let arg_ptr = slice_from_raw_parts_mut(arg_ptr, d);

				arg_ptr.write_from_buffer(&out).map_err(|_| Error::InvalidPointer)?;
				Ok(res)
			}
			_ => { todo!() }
		}
	}
}

pub fn open(path: &str) -> Result<Handle, Error> {
	let (domain, endpoint) = match <char as Pattern>::into_searcher(':', path).next_match() {
		Some((begin, end)) => {
			(path.get(..begin).unwrap(), path.get(end..).unwrap())
		},
		None => ("", path),
	};

	let endpoint = endpoint.trim_start_matches('/')
	                       .trim_end_matches('/');

	let domain = domain.trim_end_matches('.');

	info!("Open `{domain}:{endpoint}`");

	let (srv_id, srv) = server::servers().get_server_at(domain)?;
	debug!("Open `{srv_id:?}:{endpoint}`");
	srv.open(Cow::Borrowed(endpoint)) // fixme: pass the Box here since the userspace thunk puts it back into a Box (and we want allocation reuse)
	   .map(|id| Handle::new(srv_id, id))
}
