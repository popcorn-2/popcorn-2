use crate::prelude::*;
use core::num::NonZero;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::{new_mapping_in, Protection};
use kernel_api::sync::OnceLock;
use crate::ipc::{Error, protocol};
use crate::ipc::ctor::{CtorContext, ProtocolVisitor};
use crate::ipc::protocol::DispatchTable;
use crate::ipc::server::Server;
use crate::memory::r#virtual::AddressSpaceInner;
use crate::threading::ThreadId;

/// Manages thread objects, implementing `core.proc.Proc` and `core.proc.Thread`
///
/// Objects are implicitly opened by thread creation
///
/// Handle value is equal to [`ThreadId`](crate::threading::ThreadId)
#[derive(Debug)]
pub struct ProcServer;

impl Server for ProcServer {
	type CtorContext = CtorCtx;

	fn ctor(&self, endpoint: &str, ctx: Self::CtorContext) -> Result<isize, Error> {
		Err(Error::UnsupportedProtocol)
	}

	fn destroy(&self, _handle: isize) -> Result<(), Error> { Ok(()) }

	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH_TABLE: OnceLock<DispatchTable> = OnceLock::new();
		DISPATCH_TABLE.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as protocol::generated::CoreProcThread>::__vtable())
		)
	}
}

fn unsupported_other_thread(handle: isize) -> Result<(), Error> {
	if handle as usize != percpu_v2!(current_thread).read().as_ref().unwrap().tcb_ref().thread_id.get() {
		Err(Error::UnsupportedProtocol)
	} else {
		Ok(())
	}
}

impl protocol::generated::CoreProcThread for ProcServer {
	fn unstable_anon_alloc(&self, handle: isize, size: usize) -> Result<*const u8, Error> {
		unsupported_other_thread(handle)?;

		let handle = handle as usize;

		let Some(len) = NonZero::new(size) else { return Ok(core::ptr::null()); };
		let len = len.div_ceil(NonZero::new(4096).unwrap());

		let guard = crate::threading::get_thread(ThreadId::new_from(handle.try_into().map_err(|_| Error::InvalidHandle)?))
				.ok_or(Error::InvalidHandle)?;
		let address_space = guard.tcb_ref().address_space;

		let config = mapping::Config::new_in(len.try_into().unwrap(), AddressSpaceInner::to_api(address_space))
				.protection(Protection::RWXU);
		let Ok(mapping) = new_mapping_in(config, u16::MAX) else { return Err(Error::Overflow); };

		let ret = mapping.virtual_start().as_ptr();

		address_space.add_mapping("[mmap]", mapping);
		trace!("{address_space:?}");

		Ok(ret.cast_const())
	}

	fn unstable_anon_dealloc(&self, handle: isize, pointer: *const u8) -> Result<(), Error> {
		unsupported_other_thread(handle)?;

		warn!("ignoring thread dealloc request of {pointer:#p}");

		Ok(())
	}

	fn set_tcb(&self, handle: isize, pointer: *const u8) -> Result<(), Error> {
		unsupported_other_thread(handle)?;
		debug!("load tcb with {pointer:#p}");
		crate::hal::load_user_tls(pointer.cast_mut());
		Ok(())
	}
}


#[derive(Default)]
pub struct CtorCtx;

impl CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> { const { &ProtocolVisitor::new() } }
}
