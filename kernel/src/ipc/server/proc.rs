use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use crate::prelude::*;
use core::num::NonZero;
use slab::Slab;
use kernel_api::memory::{mapping, Page, VirtualAddress};
use kernel_api::memory::mapping::{Location, new_mapping_in, new_stack_in, Protection, Stack};
use kernel_api::memory::r#virtual::Userspace;
use kernel_api::sync::{OnceLock, Spinlock};
use crate::ipc::{Error, protocol};
use crate::ipc::ctor::{CtorContext, ProtocolVisitor};
use crate::ipc::dispatch::Return;
use crate::ipc::handle::{Handle, HandleMap};
use crate::ipc::protocol::{DispatchTable, Protocol};
use crate::ipc::protocol::generated::{CoreIoRead, CoreIoSeek};
use crate::ipc::server::{ReturnHandle, Server};
use crate::memory::r#virtual::AddressSpaceInner;
use crate::threading;
use crate::threading::ThreadId;

/// Manages thread objects, implementing `core.proc.Proc` and `core.proc.Thread`
///
/// Objects are implicitly opened by thread creation
///
/// Handle value is equal to [`ThreadId`]
#[derive(Debug)]
pub struct ProcServer {
	builders: Spinlock<Slab<UnspawnedThread>>,
}

impl ProcServer {
	pub const fn new() -> Self {
		ProcServer {
			builders: Spinlock::new(Slab::new())
		}
	}
}

impl Server for ProcServer {
	type CtorContext = CtorCtx;

	fn ctor(&self, endpoint: &str, ctx: Self::CtorContext) -> Result<ReturnHandle, Error> {
		Err(Error::UnsupportedProtocol)
	}

	fn destroy(&self, _handle: isize) -> Result<(), Error> { Ok(()) }

	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH_TABLE: OnceLock<DispatchTable> = OnceLock::new();
		DISPATCH_TABLE.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as protocol::generated::CoreProcThread>::__vtable())
				.add_vtable(<Self as protocol::generated::CoreProcBuilder>::__vtable())
		)
	}
}

fn unsupported_other_thread(handle: isize) -> Result<(), Error> {
	if handle != percpu_v2!(current_thread).read().as_ref().unwrap().tcb_ref().thread_id.get() {
		Err(Error::UnsupportedProtocol)
	} else {
		Ok(())
	}
}

impl protocol::generated::CoreProcThread for ProcServer {
	async fn new_from(&self, _: &str, _: Arc<Handle>) -> Result<ReturnHandle, Error> { Err(Error::UnsupportedProtocol) }

	async fn unstable_anon_alloc(&self, handle: isize, size: usize) -> Result<*const u8, Error> {
		unsupported_other_thread(handle)?;

		let Some(len) = NonZero::new(size) else { return Ok(core::ptr::null()); };
		let len = len.div_ceil(NonZero::new(4096).unwrap());

		let guard = threading::get_thread(ThreadId::new_from(handle.try_into().map_err(|_| Error::InvalidHandle)?))
				.ok_or(Error::InvalidHandle)?;
		let address_space = guard.tcb_ref().address_space;

		let config = mapping::Config::new_in(len.try_into().unwrap(), AddressSpaceInner::to_api(address_space))
				.protection(Protection::RWXU);
		let Ok(mapping) = new_mapping_in(config, u16::MAX) else { return Err(Error::Overflow); };

		let ret = mapping.virtual_valid_start().as_ptr();

		address_space.add_mapping("[anon mmap]", mapping);
		trace!("{address_space:?}");

		Ok(ret.cast_const())
	}

	async fn unstable_anon_dealloc(&self, handle: isize, pointer: *const u8) -> Result<(), Error> {
		unsupported_other_thread(handle)?;

		warn!("ignoring thread dealloc request of {pointer:#p}");

		Ok(())
	}

	async fn set_tcb(&self, handle: isize, pointer: *const u8) -> Result<(), Error> {
		unsupported_other_thread(handle)?;
		debug!("load tcb with {pointer:#p}");
		crate::hal::load_user_tls(pointer.cast_mut());
		Ok(())
	}

	async fn spawn_thread(&self, handle: isize, name: &str, stack_top: *const u8, entry: *const u8) -> Result<ReturnHandle, Error> {
		unsupported_other_thread(handle)?;
		
		let entry = VirtualAddress::from(entry);
		let stack_top = VirtualAddress::from(stack_top);

		threading::clone_current(move || crate::hal::switch_to_userspace_at(entry, stack_top), Cow::Owned(name.to_owned()))
				.map(|tid| ReturnHandle::New(
					tid.get(),
					Box::from([<dyn protocol::generated::CoreProcThread>::UID]),
				)).map_err(Error::from)
	}

	async fn yield_now(&self, handle: isize) -> Result<(), Error> {
		unsupported_other_thread(handle)?;
		
		let _ = threading::yield_now();
		Ok(())
	}
}

impl protocol::generated::CoreProcBuilder for ProcServer {
	async fn spawn(&self, handle: isize) -> Result<ReturnHandle, Error> {
		debug_assert!(handle < 0, "builder handles should have MSB set");
		let key = handle.unsigned_abs() - 1;
		let builder = self.builders.lock()
				.try_remove(key)
				.ok_or(Error::InvalidHandle)?;

		let UnspawnedThread {
			name,
			address_space,
			entry,
			handle_map,
			handle_nums
		} = builder;
		
		let config = mapping::Config::new_in(NonZero::new(8).unwrap(), AddressSpaceInner::to_api(&address_space))
				.virtual_location(Location::At(Page::new(VirtualAddress::new(0x40000000))))
				.protection(Protection::RWXU);

		let stack = new_stack_in(config, u16::MAX)?;

		let stack_top = crate::loader::set_up_stack(&stack, [], [], handle_nums);
				
		address_space.add_mapping("[stack]", stack);

		let tid = threading::spawn_into(
			move || crate::hal::switch_to_userspace_at(entry, stack_top),
			Cow::Owned(name),
			address_space,
			handle_map,
		)?;
		
		Ok(ReturnHandle::New(
			tid.get(),
			Box::from([<dyn protocol::generated::CoreProcThread>::UID]),
		))
	}

	async fn add_handle(&self, handle: isize, name: &str, add_handle: Arc<Handle>) -> Result<(), Error> {
		debug_assert!(handle < 0, "builder handles should have MSB set");
		let key = handle.unsigned_abs() - 1;
		let mut guard = self.builders.lock();
		let builder = guard
		                  .get_mut(key)
		                  .ok_or(Error::InvalidHandle)?;
		
		let name = Box::from(name);
		let num = builder.handle_map.push(add_handle)?;
		builder.handle_nums.insert(name, num);
		
		Ok(())
	}

	async fn new_from(&self, endpoint: &str, handle: Arc<Handle>) -> Result<ReturnHandle, Error> {
		info!("spawn process `{endpoint}` with handle {handle:#x?}");

		if !handle.has_protocols(&[<dyn CoreIoRead>::UID, <dyn CoreIoSeek>::UID]) { return Err(Error::InvalidArg); }
		
		let builder = UnspawnedThread {
			name: endpoint.to_owned(),
			address_space: AddressSpaceInner::empty()?,
			entry: VirtualAddress::new(0),
			handle_nums: BTreeMap::new(),
			handle_map: HandleMap::new(),
		};
		
		let key = self.builders.lock().insert(builder);
		let handle = -((key + 1) as isize);
		info!("spawn builder with handle {handle}");
		if handle >= 0 {
			self.builders.lock().remove(key);
			Err(Error::AllocationFailure)
		} else {
			Ok(ReturnHandle::NewDefault(handle))
		}
	}
}

#[derive(Default)]
pub struct CtorCtx;

impl CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> { const { &ProtocolVisitor::new() } }
}

#[derive(Debug)]
struct UnspawnedThread {
	name: String,
	address_space: Arc<AddressSpaceInner>,
	entry: VirtualAddress,
	handle_nums: BTreeMap<Box<str>, u32>,
	handle_map: HandleMap,
}
