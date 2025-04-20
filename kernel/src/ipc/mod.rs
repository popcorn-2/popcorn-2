#[allow(unused_imports)] use crate::prelude::*;

pub mod server;
pub mod handle;
mod protocol;

use utils::better_cow::Cow;
use kernel_api::ptr::slice_from_raw_parts;
use core::str::pattern::{Pattern, Searcher};
use hashbrown::HashMap;
use kernel_api::ptr::User;
use kernel_api::sync::RwSpinlock;
use crate::ipc::handle::Handle;
use crate::ipc::protocol::Method;
use server::Server as _;
use crate::memory::r#virtual::AddressSpaceInner;

static METHODS: RwSpinlock<HashMap<u128, Method>> = RwSpinlock::new(HashMap::with_hasher(hashbrown::hash_map::DefaultHashBuilder::new()));

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

fn syscall(proto_method: u128, a: usize, b: usize, _c: usize, _d: usize) -> Result<NonNegativeIsize, Error> {
	if proto_method == 0 /* open@core.socket */ {
		// For now, we special case `open` as the only static function (not taking a `Handle`, and
		// thus requiring extra knowledge by the kernel to know where to dispatch it)
		// In future, when static methods are fully supported this will be modified to become
		// more generic

		let ptr = {
			let endpoint_ptr = User::<*const u8>::new_in(
				a as _,
				AddressSpaceInner::to_api(
					percpu_v2!(current_thread).read().as_ref().expect("can only syscall from thread")
							.tcb_ref().address_space
				),
			);
			slice_from_raw_parts(endpoint_ptr, b)
		};

		open(ptr).and_then(|handle| {
			let res = percpu_v2!(current_thread).read().as_ref()
					.expect("must be running on a thread to syscall")
					.tcb_ref().handles.push(handle);
			res.map(|val| NonNegativeIsize::new(val.try_into().unwrap()).unwrap())
		})
	} else {
		let Some(meta) = METHODS.read().get(&proto_method).map(|&x| x) else {
			yeet!(Error::Unimplemented);
		};
		
		let handle = percpu_v2!(current_thread).read().as_ref()
		                                       .expect("must be running on a thread to syscall")
		                                       .tcb_ref().handles.get(a.try_into().unwrap())?;

		todo!()
	}
}

fn open(endpoint: User<*const [u8]>) -> Result<Handle, Error> {
	let Ok(buf) = (unsafe { endpoint.read_to_buffer() }) else { yeet!(Error::InvalidPointer); };

	let path = match core::str::from_utf8(&buf) {
		Ok(path) => path,
		Err(e) => {
			error!("{buf:#x?}\n{e:?}");
			yeet!(Error::InvalidUtf8);
		},
	};

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

	let (srv_id, srv) = server::servers().get_server(domain)?;
	srv.open(Cow::Borrowed(endpoint)) // fixme: pass the Box here since the userspace thunk puts it back into a Box (and we want allocation reuse)
			.map(|id| Handle::new(srv_id, id))
}
