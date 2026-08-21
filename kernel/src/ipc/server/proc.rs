use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::future::Future;
use core::hint::unreachable_unchecked;
use core::mem::MaybeUninit;
use core::num::NonZero;
use core::ptr;
use slab::Slab;
use elf::{Endianness, Isa, Type, Width, FileHeader};
use elf::segment::{Segment, Flags as SegmentFlags, Type as SegmentType};
use kernel_api::address_space::AddressSpace;
use kernel_api::executor::block_on;
use kernel_api::mapping::{Caching, Config, MappingMeta, Mmap, Stack, Ty, UnsafeMmap};
use kernel_api::memory::{PAGE_SIZE, PhysicalAddress, VirtualAddress, RawPage};
use kernel_api::sync::{OnceLock, Spinlock};
use kernel_api::syscall;
use kernel_api::syscall::AsyncMap;
use crate::ipc::{Error, protocol, server};
use crate::ipc::ctor::{CtorContext, ProtocolVisitor};
use kernel_api::syscall::handle::{Handle, HandleMap};
use crate::ipc::protocol::{DispatchTable, Protocol};
use crate::ipc::protocol::generated::core::io::{Read, Seek};
use crate::ipc::server::{Either, ReturnHandle, Server};
use crate::{hal, threading};
use kernel_api::threading::{ThreadId, ThreadMeta};
use alloc::borrow::Cow;

/// Manages thread objects, implementing `core.proc.Proc` and `core.proc.Thread`
///
/// Objects are implicitly opened by thread creation
#[derive(Debug)]
pub struct ProcServer {
	threads: Spinlock<Slab<Thread>>,
}

#[derive(Debug)]
enum Thread {
	Building {
		meta: Arc<ThreadMeta>,
		startup: oneshot::Sender<StartupPacket>,
		handle_nums: BTreeMap<Box<str>, u32>,
		entry: Option<VirtualAddress>,
		env_vars: Vec<Box<str>>,
		args: Vec<Box<str>>,
	},
	Running(Arc<ThreadMeta>),
}

#[derive(Debug)]
struct StartupPacket {
	stack_top: VirtualAddress,
	entry_override: Option<VirtualAddress>,
}

impl ProcServer {
	pub fn new(tid0: Arc<ThreadMeta>) -> Self {
		let mut threads = Slab::new();
		assert_eq!(threads.vacant_key(), tid0.thread_id.get() as usize);
		threads.insert(Thread::Running(tid0));
		ProcServer {
			threads: Spinlock::new(threads),
		}
	}
}

impl Server for ProcServer {
	type CtorContext = CtorCtx;

	async fn ctor(&self, _endpoint: &str, _ctx: Self::CtorContext) -> Result<ReturnHandle, Error> {
		Err(Error::UnsupportedProtocol)
	}

	async fn destroy(&self, handle: isize) -> Result<(), Error> {
		warn!("proc server todo drop handle {handle:#x}");
		Err(Error::UnsupportedProtocol)
	}

	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH_TABLE: OnceLock<DispatchTable> = OnceLock::new();
		DISPATCH_TABLE.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as protocol::generated::core::proc::Thread>::__vtable())
				.add_vtable(<Self as protocol::generated::core::proc::Builder>::__vtable())
		)
	}
}

fn unsupported_other_thread(handle: isize) -> Result<(), Error> {
	let tid = percpu_v2!(current_thread)
			.read()
			.as_ref()
			.expect("cannot syscall from idle")
			.thread_id;
	if handle != tid.get() {
		Err(Error::UnsupportedProtocol)
	} else {
		Ok(())
	}
}

impl protocol::generated::core::proc::Thread for ProcServer {
	async fn unstable_anon_alloc(&self, handle: isize, size: usize) -> Result<*const u8, Error> {
		let Some(len) = NonZero::new(size) else { return Ok(core::ptr::null()); };
		let len = len.div_ceil(NonZero::new(PAGE_SIZE).unwrap());

		let guard = self.threads.lock();
		let thread = match guard.get(handle as usize).ok_or(Error::InvalidHandle)? {
			Thread::Building { meta, .. } => meta,
			Thread::Running(meta) => meta,
		};

		let ret = {
			let MappingMeta { mapping, ..} = Config::new(len, Ty::USER_MMAP)
					.protection(true, false, true)
					.map_in::<Mmap>("".into(), &thread.address_space)?;

			let ret = mapping.as_ptr();

			Ok(ret.addr().as_ptr().cast_const())
		};

		debug!("{:?}", thread.address_space);

		ret
	}

	fn unstable_anon_dealloc(&self, handle: isize, pointer: *const u8) -> impl Future<Output = Result<(), Error>> {
		match unsupported_other_thread(handle) {
			Err(e) => return Either::Left(core::future::ready(Err(e))),
			_ => {}
		}

		warn!("ignoring thread dealloc request of {pointer:#p}");

		Either::Right(core::future::ready(Ok(())))
	}

	fn set_tcb(&self, handle: isize, pointer: *const u8) -> impl Future<Output = Result<(), Error>> {
		match unsupported_other_thread(handle) {
			Err(e) => return Either::Left(core::future::ready(Err(e))),
			_ => {}
		}

		debug!("load tcb with {pointer:#p}");
		hal::load_user_tls(pointer.cast_mut());
		Either::Right(core::future::ready(Ok(())))
	}

	fn spawn_thread(&self, handle: isize, name: &str, stack_top: *const u8, entry: *const u8) -> impl Future<Output = Result<ReturnHandle, Error>> {
		let res = try {
			unsupported_other_thread(handle)?;

			let entry = VirtualAddress::from(entry);
			let stack_top = VirtualAddress::from(stack_top);

			let mut thread_guard = self.threads.lock();
			let (address_space, handle_map, async_map) = {
				let parent = match thread_guard.get(handle as usize).ok_or(Error::InvalidHandle)? {
					Thread::Building { .. } => Err(Error::UnsupportedProtocol)?,
					Thread::Running(meta) => meta,
				};
				(
					AddressSpace::clone(&parent.address_space),
					HandleMap::clone(&parent.handles),
					Arc::clone(&parent.async_map),
				)
			};

			let thread_entry = thread_guard.vacant_entry();
			if (thread_entry.key() as isize) < 0 { Err(Error::Overflow)?; };
			let tid = thread_entry.key() as isize;

			let meta = threading::spawn(
				Arc::from(name),
				address_space,
				ThreadId::new(tid),
				handle_map,
				async_map,
				move || hal::switch_to_userspace_at(entry, stack_top),
			).map_err(From::from)?;

			thread_entry.insert(Thread::Running(meta));
			ReturnHandle::New(
				tid,
				Box::from([<dyn protocol::generated::core::proc::Thread>::UID]),
				name.to_owned().into(),
			)
		};
		core::future::ready(res)
	}

	async fn exit(&self, handle: isize, code: isize) -> Result<(), Error> {
		unsupported_other_thread(handle)?;

		threading::exit(code)
	}

	async fn join(&self, handle: isize) -> Result<isize, Error> {
		let thread = {
			let guard = self.threads.lock();
			let thread = match guard.get(handle as usize).ok_or(Error::InvalidHandle)? {
				Thread::Building { meta, .. } => meta,
				Thread::Running(meta) => meta,
			};

			// todo: check if already a thread waiting on the process

			Arc::clone(thread)
		};

		let res = thread.join().await;

		let _ = self.threads.lock().try_remove(handle as usize);

		Ok(res)
	}

	async fn yield_now(&self, handle: isize) -> Result<(), Error> {
		unsupported_other_thread(handle)?;

		let _ = threading::yield_now();
		Ok(())
	}

	async fn unstable_mmio_alloc(&self, handle: isize, physical_addr: usize, size: usize) -> Result<*const u8, Error> {
		let physical_addr = PhysicalAddress::new(physical_addr);
		if !physical_addr.aligned_to(PAGE_SIZE) { return Err(Error::InvalidArg); }

		let Some(len) = NonZero::new(size) else { return Ok(core::ptr::null()); };
		let len = len.div_ceil(NonZero::new(PAGE_SIZE).unwrap());

		let guard = self.threads.lock();
		let thread = match guard.get(handle as usize).ok_or(Error::InvalidHandle)?  {
			Thread::Building { meta, .. } => meta,
			Thread::Running(meta) => meta,
		};

		let ret = {
			let MappingMeta { mapping, ..} = Config::new(len, Ty::USER_MMIO)
					.physical_location(physical_addr.align_down_to_frame())
					.with_allocator(&hal::acpi::Allocator)
					.protection(true, false, true)
					.caching(Caching::Mmio)
					.map_in::<Mmap>("mmio_legacy".into(), &thread.address_space)?;

			let ret = mapping.as_ptr();

			Ok(ret.addr().as_ptr().cast_const())
		};

		debug!("{:?}", thread.address_space);

		ret
	}

	fn map_vmo(&self, handle: isize, vmo: Arc<Handle>, address: *const u8, len: usize, offset: usize) -> impl Future<Output = Result<*const u8, Error>> {
		let address = address.addr();
		async move {
			if len % PAGE_SIZE != 0 { return Err(Error::InvalidArg); }
			if offset % PAGE_SIZE != 0 { return Err(Error::InvalidArg); }
			if address != 0 { return Err(Error::FutureCompat); }

			let Some(len) = NonZero::new(len) else { return Ok(ptr::null()); };
			let len = len.div_ceil(NonZero::new(PAGE_SIZE).unwrap());

			let guard = self.threads.lock();
			let thread = match guard.get(handle as usize).ok_or(Error::InvalidHandle)? {
				Thread::Building { meta, .. } => meta,
				Thread::Running(meta) => meta,
			};

			let ret = {
				let name = String::from(&**vmo.endpoint());
				let MappingMeta { mapping, ..} = Config::new(len, Ty::USER_MMAP)
					.protection(true, false, true)
					.with_vmo(vmo, offset)
					.map_in::<Mmap>(Cow::Owned(name), &thread.address_space)?;

				let ret = mapping.as_ptr();

				Ok(ret.addr().as_ptr().cast_const())
			};

			trace!("{:?}", thread.address_space);

			ret
		}
	}

	async fn new_from(&self, _: &str, _: Arc<Handle>) -> Result<ReturnHandle, Error> { Err(Error::UnsupportedProtocol) }
}

impl protocol::generated::core::proc::Builder for ProcServer {
	async fn spawn(&self, handle: isize) -> Result<ReturnHandle, Error> {
		let mut guard = self.threads.lock();

		let Some(thread @ Thread::Building { .. }) = guard.get_mut(handle as usize) else {
			return Err(Error::UnsupportedProtocol);
		};

		let Thread::Building {
			meta,
			startup,
			mut handle_nums,
			env_vars,
			args,
			entry,
		} = (unsafe { ptr::read(thread) }) else { unsafe { unreachable_unchecked(); } };
		
		// fixme: hacky
		let server_id = server::server_registry().name_lookup.get("proc").expect("proc server must exist").clone();
		// fixme: should `thread.main == 3` be an abi guarantee?
		let main_thread_handle = meta.handles.openat(3, Handle::new(
			server_id,
			handle,
			&[<dyn protocol::generated::core::proc::Thread>::UID],
			"",
		))?;
		handle_nums.insert(Box::from("thread.main"), main_thread_handle);

		let stack_top = {
			let MappingMeta { mapping: mut stack, ..} = Config::new(NonZero::new(128).unwrap(), Ty::USER_STACK)
					.protection(true, false, true)
					.virtual_location(RawPage::new(0x40000000))
					.map_in::<Stack>(format!("[stack:{main_thread_handle}]").into(), &meta.address_space)?;

			crate::loader::set_up_stack(&mut stack, args, env_vars, handle_nums)
		};

		unsafe {
			ptr::write(
				thread,
				Thread::Running(meta),
			);
		}

		startup.send(StartupPacket { stack_top, entry_override: entry }).unwrap();
		
		Ok(ReturnHandle::New(
			handle,
			Box::from([<dyn protocol::generated::core::proc::Thread>::UID]),
			"[thread]".into(),
		))
	}

	async fn add_handle(&self, handle: isize, name: &str, add_handle: Arc<Handle>) -> Result<(), Error> {
		let mut guard = self.threads.lock();
		debug!("add handle to {handle:#x} ({:#x?})", guard.get_mut(handle as usize));
		let Some(Thread::Building { meta, handle_nums, .. }) = guard.get_mut(handle as usize) else {
			return Err(Error::UnsupportedProtocol);
		};
		
		let name = Box::from(name);
		let num = if &*name == "io.stdin" {
			meta.handles.openat(0, add_handle)?;
			0
		} else if &*name == "io.stdout" {
			meta.handles.openat(1, add_handle)?;
			1
		} else if &*name == "io.stderr" {
			meta.handles.openat(2, add_handle)?;
			2
		} else {
			meta.handles.push(add_handle)?
		};
		
		handle_nums.insert(name, num);
		
		Ok(())
	}

	async fn add_env_var(&self, handle: isize, val: &str) -> Result<(), Error> {
		let mut guard = self.threads.lock();
		debug!("add env_var to {handle:#x} ({:#x?})", guard.get_mut(handle as usize));
		let Some(Thread::Building { env_vars, .. }) = guard.get_mut(handle as usize) else {
			return Err(Error::UnsupportedProtocol);
		};

		env_vars.push(Box::from(val));

		Ok(())
	}

	async fn add_arg(&self, handle: isize, val: &str) -> Result<(), Error> {
		let mut guard = self.threads.lock();
		debug!("add arg to {handle:#x} ({:#x?})", guard.get_mut(handle as usize));
		let Some(Thread::Building { args, .. }) = guard.get_mut(handle as usize) else {
			return Err(Error::UnsupportedProtocol);
		};

		args.push(Box::from(val));

		Ok(())
	}

	async fn new_from(&self, endpoint: &str, handle: Arc<Handle>) -> Result<ReturnHandle, Error> {
		info!("spawn process `{endpoint}` with handle {handle:#x?}");

		async fn read_elf_header_from(handle: &Arc<Handle>) -> syscall::Result<FileHeader> {
			if !handle.has_protocols(&[<dyn Read>::UID, <dyn Seek>::UID]) { return Err(Error::InvalidArg); }

			let mut raw_header = MaybeUninit::<[u8; elf::raw::file::x64::SIZE]>::uninit();

			let res = handle.kernel_syscall(
				<dyn Read>::UID,
				1,
				[
					raw_header.as_mut_ptr().addr(),
					elf::raw::file::x64::SIZE,
					0,
					0,
				]
			).await?;

			debug!("read {res} bytes from ELF file");
			if (res as usize) < elf::raw::file::x64::SIZE { return Err(Error::EndOfData); }
			let header = FileHeader::try_new(unsafe { raw_header.assume_init_ref() });
			info!("elf header: {header:#?}");
			let header = header.map_err(|e| {
				debug!("failed to parse ELF header: {e:?}");
				Error::InvalidArg
			})?;
			if header.width() != Width::X64 {
				debug!("not 64 bit");
				return Err(Error::InvalidArg);
			}
			if header.endianness() != Endianness::LITTLE {
				debug!("not little endian");
				return Err(Error::InvalidArg);
			}
			if header.isa() != Isa::X86_64 {
				debug!("not amd64");
				return Err(Error::InvalidArg);
			}

			// move out of a MaybeUninit which we don't use again
			Ok(header)
		}

		async fn load_elf_from_with_offset(handle: &Arc<Handle>, header: &FileHeader, address_space: &AddressSpace, base: Option<VirtualAddress>) -> syscall::Result<VirtualAddress> {
			let offset = base.map(|addr| addr.addr).unwrap_or(0);

			let mut interp_handle = None;
			let mut highest_addr = VirtualAddress::new(0);

			for entry_start in header.program_header().step_by(header.program_header_entry_size().into()) {
				let _ = block_on(
					handle.kernel_syscall(
						<dyn Seek>::UID,
						2,
						[
							entry_start,
							0,
							0,
							0,
						]
					)
				)?;

				let mut raw = MaybeUninit::<[u8; elf::raw::program::x64::SIZE]>::uninit();
				let res = block_on(
					handle.kernel_syscall(
						<dyn Read>::UID,
						1,
						[
							raw.as_mut_ptr().addr(),
							elf::raw::program::x64::SIZE,
							0,
							0,
						]
					)
				)?;
				if (res as usize) < elf::raw::program::x64::SIZE { Err(Error::EndOfData)?; }
				let segment = Segment::try_new(unsafe { raw.assume_init() }, header)
					.map_err(|_| Error::InvalidArg)?;
				info!("found header {segment:#?}");

				if segment.ty() == SegmentType::LOAD {
					let _ = block_on(
						handle.kernel_syscall(
							<dyn Seek>::UID,
							2,
							[
								segment.file_offset().try_into().unwrap(),
								0,
								0,
								0,
							]
						)
					)?;

					assert_eq!(segment.align() as usize, PAGE_SIZE, "Not designed for !=1 page alignment");

					let addr = VirtualAddress::new(segment.vaddr().try_into().unwrap()) + offset;
					let segment_page_offset = addr - *addr.align_down_to_page();

					let len = segment_page_offset + usize::try_from(segment.mem_size()).unwrap();
					if let Some(len) = NonZero::new(len.div_ceil(PAGE_SIZE)) {
						let (mapping_key, page) = {
							let MappingMeta { key, mapping, .. } = Config::new(len, Ty::USER_CODE)
									.protection(
										true, // segment.segment_flags.contains(SegmentFlags::Writeable),
										segment.flags().contains(SegmentFlags::Executable),
										true
									)
									.virtual_location(addr.align_down_to_page())
									.map_in::<UnsafeMmap>(Cow::Owned(String::from(&**handle.endpoint())), &address_space)?;

							assert!(segment.file_size() <= segment.mem_size());

							if mapping.as_ptr_range().end.addr() > highest_addr {
								highest_addr = mapping.as_ptr_range().end.addr();
							}

							(key, mapping.virtual_valid_start())  // fixme: is it sound to drop the mapping here?
						};

						let res = match block_on(
							handle.kernel_syscall(
								<dyn Read>::UID,
								1,
								[
									page.as_ptr().addr() + segment_page_offset,
									segment.file_size().try_into().unwrap(),
									0,
									0,
								]
							)
						) {
							Ok(res) => res,
							e @ Err(Error::InvalidArg | Error::InvalidHandle | Error::InvalidPointer) => e.expect("args should be valid"),
							Err(e) => Err(e)?,
						};
						if (res as u64) < segment.file_size() { Err(Error::EndOfData)?; }

						let mut mapping = address_space.get(mapping_key).expect("mapping should still exist");
						let zero_start = unsafe {
							let ptr = mapping.as_mut_ptr()
							                 .byte_add(segment.file_size().try_into().unwrap())
							                 .byte_add(segment_page_offset);
							kernel_api::ptr::slice_from_raw_parts_mut(
								ptr,
								(segment.mem_size() - segment.file_size()).try_into().unwrap(),
							)
						};
						zero_start.fill(0).unwrap();

						// todo: make read-only pages actually read-only
					}
				} else if segment.ty() == SegmentType::INTERPRETER {
					let _ = block_on(
						handle.kernel_syscall(
							<dyn Seek>::UID,
							2,
							[
								segment.file_offset().try_into().unwrap(),
								0,
								0,
								0,
							]
						)
					)?;

					let mut buffer = Vec::<u8>::with_capacity(segment.file_size() as usize);

					let res = match block_on(
						handle.kernel_syscall(
							<dyn Read>::UID,
							1,
							[
								buffer.as_mut_ptr().addr(),
								segment.file_size().try_into().unwrap(),
								0,
								0,
							]
						)
					) {
						Ok(res) => res,
						e @ Err(Error::InvalidArg | Error::InvalidHandle | Error::InvalidPointer) => e.expect("args should be valid"),
						Err(e) => Err(e)?,
					};
					if (res as u64) < segment.file_size() { Err(Error::EndOfData)?; }

					unsafe { buffer.set_len(segment.file_size() as usize) };

					// chop off the NUL byte
					let interpreter_path = str::from_utf8(&buffer[..buffer.len()-1]).unwrap();
					info!("interpreter path: {}", interpreter_path);

					interp_handle = Some(block_on(crate::ipc::abi_v1::open_async(interpreter_path, &[<dyn Read>::UID, <dyn Seek>::UID]))?);
				}
			}

			if let Some(interp_handle) = interp_handle {
				let base = highest_addr.align_up_to_page() + 8usize;
				info!("loading interpreter at {base:#x}");

				let interp_header = block_on(read_elf_header_from(&interp_handle))?;

				if interp_header.ty() != Type::SHARED {
					warn!("not shared library");
					return Err(Error::InvalidArg);
				}

				Box::pin(load_elf_from_with_offset(
					&interp_handle,
					&interp_header,
					address_space,
					Some(*base),
				)).await
			} else {
				Ok(VirtualAddress::new(header.entry_point()) + offset)
			}
		}

		let address_space = AddressSpace::empty()?;

		let header = read_elf_header_from(&handle).await?;
		if header.ty() != Type::EXECUTABLE {
			warn!("not executable");
			return Err(Error::InvalidArg);
		}

		let (send_startup, receive_startup) = oneshot::channel::<StartupPacket>();
		let (send_setup, receive_setup) = oneshot::channel();

		let tid = {
			let mut thread_guard = self.threads.lock();
			let thread_entry = thread_guard.vacant_entry();
			if (thread_entry.key() as isize) < 0 { Err(Error::Overflow)?; };
			let tid = thread_entry.key() as isize;

			let endpoint = endpoint.to_owned();
			let meta = threading::spawn(
				endpoint.into(),
				AddressSpace::clone(&address_space),
				ThreadId::new(tid),
				HandleMap::new(),
				Arc::new(AsyncMap::new()),
				move || {
					let entrypoint = block_on(load_elf_from_with_offset(
						&handle,
						&header,
						&address_space,
						None,
					));

					debug!("program load done");

					send_setup.send(entrypoint.map(|_| ())).unwrap();

					if entrypoint.is_err() { return; }

					let startup = block_on(receive_startup).unwrap();
					let entrypoint = match startup.entry_override {
						Some(entrypoint) => entrypoint,
						None => entrypoint.expect("should not have received startup packet if loading errored"),
					};

					hal::switch_to_userspace_at(entrypoint, startup.stack_top);
				}
			)?;

			thread_entry.insert(Thread::Building {
				meta,
				startup: send_startup,
				handle_nums: BTreeMap::new(),
				env_vars: vec![],
				args: vec![],
				entry: None,
			});

			tid
		};

		match receive_setup.await.expect("thread should be set up") {
			Ok(_) => Ok(ReturnHandle::NewDefault(tid)),
			Err(e) => {
				let _ = self.threads.lock()
						.remove(tid as usize);
				Err(e)
			}
		}
	}
}

#[derive(Default)]
pub struct CtorCtx;

impl CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> { const { &ProtocolVisitor::new() } }
}

