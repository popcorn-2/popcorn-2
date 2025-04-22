use ::core::num::NonZero;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::Mapping;
#[allow(unused_imports)] use crate::prelude::*;
use utils::better_cow::Cow;
use crate::ipc::{Error, NonNegativeIsize};
use crate::ipc::server::Server;
use crate::memory::r#virtual::AddressSpaceInner;
use super::super::core;

/// Manages thread objects, implementing `core.proc.Proc` and `core.proc.Thread`
/// 
/// Objects are implicitly opened by thread creation
/// 
/// Handle value is equal to [`ThreadId`](crate::threading::ThreadId)
#[derive(Debug)]
pub struct ProcServer {}

impl Server for ProcServer {
	fn open(&self, _endpoint: Cow<'_, Box<str>, str>) -> Result<usize, Error> {
		unimplemented!()
	}

	fn dispatch_vvv_ve(&self, proto_method: u128, fd: usize, b: usize, c: usize, d: usize) -> Result<NonNegativeIsize, Error> {
		if fd != percpu_v2!(current_thread).read().as_ref().unwrap().tcb_ref().thread_id.get() { return Err(Error::Unimplemented); }
		
		match proto_method {
			const { core::proc::THREAD | core::proc::THREAD_SET_TCB } => {
				unsafe { crate::hal::load_user_tls(b as _); }
				return Ok(NonNegativeIsize::new(0).unwrap());
			}
			const { core::proc::PROC | core::proc::PROC_EXIT } => {
				crate::threading::exit(b as i8);
			}
			const { core::proc::PROC | core::proc::PROC_ALLOC } => {
				let Some(len) = NonZero::new(b) else { return Ok(NonNegativeIsize::new(0).unwrap()); };
				let len = len.div_ceil(NonZero::new(4096).unwrap());

				let guard = percpu_v2!(current_thread).read();
				let address_space = guard.as_ref().unwrap().tcb_ref().address_space;

				let config = mapping::Config::new_in(len.try_into().unwrap(), AddressSpaceInner::to_api(address_space));
				let Ok(mapping) = Mapping::new_in(config, u16::MAX) else { return Err(Error::Overflow); };

				let ret = mapping.virtual_start().as_ptr() as isize;

				address_space.add_mapping("[mmap]", mapping);
				trace!("{address_space:?}");

				Ok(NonNegativeIsize::new(ret).unwrap())
			}
			const { core::proc::PROC | core::proc::PROC_DEALLOC } => { Ok(NonNegativeIsize::new(0).unwrap()) }
			_ => unimplemented!()
		}
	}

	fn dispatch_vs_ve(&self, proto_method: u128, fd: usize, b: usize, s: String) -> Result<NonNegativeIsize, Error> {
		match proto_method {
			const { core::proc::PROC | core::proc::PROC_DEBUG } => {
				info!("log(ThreadId({fd})) - `{s}`");
				Ok(NonNegativeIsize::new(0).unwrap())
			}
			_ => unimplemented!()
		}
	}
}

impl ProcServer {
	pub const fn new() -> Self {
		Self {}
	}
}
