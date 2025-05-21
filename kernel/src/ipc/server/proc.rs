use ::core::num::NonZero;
use core::sync::atomic::Ordering;
use kernel_api::memory::{mapping, VirtualAddress};
use kernel_api::memory::mapping::{Mapping, new_mapping_in, Protection};
#[allow(unused_imports)] use crate::prelude::*;
use utils::better_cow::Cow;
use crate::ipc::{Error, NonNegativeIsize};
use crate::ipc::server::Server;
use crate::memory::r#virtual::AddressSpaceInner;
use super::super::core_protos;
use crate::hal::SaveStateTr;
use crate::threading::ThreadId;

/// Manages thread objects, implementing `core.proc.Proc` and `core.proc.Thread`
/// 
/// Objects are implicitly opened by thread creation
/// 
/// Handle value is equal to [`ThreadId`](crate::threading::ThreadId)
#[derive(Debug)]
pub struct ProcServer {}

impl Server for ProcServer {
	fn open(&self, endpoint: Cow<'_, Box<str>, str>) -> Result<usize, Error> {
		let endpoint = &*endpoint;
		if let Some(thread_name) = endpoint.strip_prefix("threads/") {
			debug!("Create child thread with name `{thread_name}`");
			let new_tcb = crate::threading::clone_current_uninit(
				alloc::borrow::Cow::Owned(thread_name.to_owned()),
			);
			new_tcb.map(|thread_id| thread_id.get())
					.map_err(|_| Error::AllocationFailure)
		} else {
			Err(Error::Unimplemented)
		}
	}

	fn dispatch_vvv_ve(&self, proto_method: u128, fd: usize, b: usize, c: usize, d: usize) -> Result<NonNegativeIsize, Error> {
		let unsupported_other_thread = || if fd != percpu_v2!(current_thread).read().as_ref().unwrap().tcb_ref().thread_id.get() { Err(Error::Unimplemented) } else { Ok(()) };

		match proto_method {
			m if m == const { core_protos::proc::THREAD | core_protos::proc::THREAD_SET_TCB } => {
				unsupported_other_thread()?;
				unsafe { crate::hal::load_user_tls(b as _); }
				return Ok(NonNegativeIsize::new(0).unwrap());
			}
			m if m == const { core_protos::proc::THREAD | core_protos::proc::THREAD_EXEC } => {
				let mut guard = crate::threading::try_get_thread_pointer(ThreadId::new_from(fd.try_into().map_err(|_| Error::InvalidArg)?))
						.ok_or_else(|| {
							debug!("could not find ThreadPointer");
							Error::InvalidArg
						})?;
				
				let userspace_shim = move || {
					crate::hal::switch_to_userspace_at(VirtualAddress::new(b), VirtualAddress::new(c));
				};
				let boxed_userspace = Box::into_raw(Box::new(Box::new(userspace_shim) as Box<dyn FnOnce() + Send + 'static>));
				extern "C" fn shim(f: usize) -> ! {
					debug!("started uninit thrad shim");
					unsafe {
						Box::from_raw(f as *mut Box<dyn FnOnce() + Send + 'static>)();
					}
					unreachable!()
				}
				
				if !guard.tcb_ref().state.load(Ordering::SeqCst).is_uninit() {
					debug!("attempt to `exec` an already running thread");
					return Err(Error::InvalidArg);
				}
				// SAFETY: We just checked the thread is still in an `Uninit` state and therefore has never been run
				unsafe {
					guard.tcb_mut().save_state.set_entry(shim, boxed_userspace as usize);
				}
				
				crate::threading::start_uninit_thread(guard);

				return Ok(NonNegativeIsize::new(0).unwrap());
			}
			m if m == const { core_protos::proc::THREAD | core_protos::proc::THREAD_YIELD } => {
				unsupported_other_thread()?;
				let _ = crate::threading::yield_now();
				Ok(NonNegativeIsize::new(0).unwrap())
			}
			m if m == const { core_protos::proc::PROC | core_protos::proc::PROC_EXIT } => {
				unsupported_other_thread()?;
				crate::threading::exit(b as i8);
			}
			m if m == const { core_protos::proc::PROC | core_protos::proc::PROC_ALLOC } => {
				let Some(len) = NonZero::new(b) else { return Ok(NonNegativeIsize::new(0).unwrap()); };
				let len = len.div_ceil(NonZero::new(4096).unwrap());

				let guard = crate::threading::get_thread(ThreadId::new_from(fd.try_into().map_err(|_| Error::InvalidArg)?))
						.ok_or(Error::InvalidArg)?;
				let address_space = guard.tcb_ref().address_space;

				let config = mapping::Config::new_in(len.try_into().unwrap(), AddressSpaceInner::to_api(address_space))
						.protection(Protection::RWXU);
				let Ok(mapping) = new_mapping_in(config, u16::MAX) else { return Err(Error::Overflow); };

				let ret = mapping.virtual_start().as_ptr() as isize;

				address_space.add_mapping("[mmap]", mapping);
				trace!("{address_space:?}");

				Ok(NonNegativeIsize::new(ret).unwrap())
			}
			m if m == const { core_protos::proc::PROC | core_protos::proc::PROC_DEALLOC } => {
				Ok(NonNegativeIsize::new(0).unwrap())
			}
			_ => unimplemented!()
		}
	}

	fn dispatch_vs_ve(&self, proto_method: u128, fd: usize, b: usize, s: String) -> Result<NonNegativeIsize, Error> {
		match proto_method {
			m if m == const { core_protos::proc::PROC | core_protos::proc::PROC_DEBUG } => {
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
