use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::future::Future;
use core::hint::unreachable_unchecked;
use core::mem::MaybeUninit;
use core::num::NonZero;
use core::ops::Range;
use core::ptr;
use slab::Slab;
use elf::header::file::{Endianness, FileHeader, FileHeaderRaw, Isa, Type, Width};
use elf::header::program::{ProgramHeaderEntry64, SegmentFlags, SegmentType};
use kernel_api::address_space::AddressSpace;
use kernel_api::executor::block_on;
use kernel_api::mapping::{Caching, Config, Mmap, Stack, Ty, UnsafeMmap};
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

		let (_, mapping) = Config::new(len, Ty::USER_MMAP)
				.protection(true, false, true)
				.map_in::<Mmap>("".into(), &thread.address_space)?;

		let ret = mapping.as_ptr();

		debug!("{:?}", thread.address_space);

		Ok(ret.addr().as_ptr().cast_const())
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

		let (_, mapping) = Config::new(len, Ty::USER_MMIO)
				.physical_location(physical_addr.align_down_to_frame())
				.with_allocator(&hal::acpi::Allocator)
				.protection(true, false, true)
				.caching(Caching::Mmio)
				.map_in::<Mmap>("[mmio]".into(), &thread.address_space)?;

		let ret = mapping.as_ptr();

		debug!("{:?}", thread.address_space);

		Ok(ret.addr().as_ptr().cast_const())
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

				let (_, mapping) = Config::new(len, Ty::USER_MMAP)
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
			let (_, mut stack) = Config::new(NonZero::new(128).unwrap(), Ty::USER_STACK)
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

			let mut raw_header = MaybeUninit::<FileHeaderRaw>::uninit();

			let res = handle.kernel_syscall(
				<dyn Read>::UID,
				1,
				[
					raw_header.as_mut_ptr().addr(),
					size_of::<FileHeaderRaw>(),
					0,
					0,
				]
			).await?;

			debug!("read {res} bytes from ELF file");
			if (res as usize) < size_of::<FileHeaderRaw>() { return Err(Error::EndOfData); }
			let header = <&FileHeader>::try_from(unsafe { raw_header.assume_init_ref() });
			info!("elf header: {header:#?}");
			let header: &FileHeader = header.map_err(|e| {
				debug!("failed to parse ELF header: {e:?}");
				Error::InvalidArg
			})?;
			if header.width != Width::_64 {
				debug!("not 64 bit");
				return Err(Error::InvalidArg);
			}
			if header.endianness != Endianness::Little {
				debug!("not little endian");
				return Err(Error::InvalidArg);
			}
			if header.isa != Isa::Amd64 {
				debug!("not amd64");
				return Err(Error::InvalidArg);
			}
			if header.elf_version != 1 {
				debug!("not elf v1");
				return Err(Error::InvalidArg);
			}

			// move out of a MaybeUninit which we don't use again
			Ok(unsafe { ptr::read(header) })
		}

		async fn load_elf_from_with_offset(handle: &Arc<Handle>, header: &FileHeader, address_space: &AddressSpace, base: Option<VirtualAddress>) -> syscall::Result<VirtualAddress> {
			let offset = base.map(|addr| addr.addr).unwrap_or(0);

			let Range {
				start: mut program_header_start,
				end: program_header_end,
			} = header.program_header();

			let program_header_count = (program_header_end - program_header_start) / size_of::<ProgramHeaderEntry64>();

			debug!("program header from {program_header_start}->{program_header_end} ({program_header_count})");

			let mut interp_handle = None;
			let mut highest_addr = VirtualAddress::new(0);

			for _ in 0..program_header_count {
				let _ = block_on(
					handle.kernel_syscall(
						<dyn Seek>::UID,
						2,
						[
							program_header_start,
							0,
							0,
							0,
						]
					)
				)?;

				let mut raw = MaybeUninit::<ProgramHeaderEntry64>::uninit();
				let res = block_on(
					handle.kernel_syscall(
						<dyn Read>::UID,
						1,
						[
							raw.as_mut_ptr().addr(),
							size_of::<ProgramHeaderEntry64>(),
							0,
							0,
						]
					)
				)?;
				if (res as usize) < size_of::<ProgramHeaderEntry64>() { Err(Error::EndOfData)?; }
				let segment = unsafe { raw.assume_init() };
				info!("found header {segment:#?}");

				program_header_start += size_of::<ProgramHeaderEntry64>();

				if segment.segment_type == SegmentType::LOAD {
					let _ = block_on(
						handle.kernel_syscall(
							<dyn Seek>::UID,
							2,
							[
								segment.file_location().0.start,
								0,
								0,
								0,
							]
						)
					)?;

					assert_eq!(segment.alignment as usize, PAGE_SIZE, "Not designed for !=1 page alignment");

					let addr = VirtualAddress::new(segment.vaddr.try_into().unwrap()) + offset;
					let segment_page_offset = addr - *addr.align_down_to_page();

					let len = segment_page_offset + usize::try_from(segment.memory_size).unwrap();
					if let Some(len) = NonZero::new(len.div_ceil(PAGE_SIZE)) {
						let (mapping_key, page) = {
							let (mapping_key, mapping) = Config::new(len, Ty::USER_CODE)
									.protection(
										true, // segment.segment_flags.contains(SegmentFlags::Writeable),
										segment.segment_flags.contains(SegmentFlags::Executable),
										true
									)
									.virtual_location(addr.align_down_to_page())
									.map_in::<UnsafeMmap>(Cow::Owned(String::from(&**handle.endpoint())), &address_space)?;

							assert!(segment.file_size <= segment.memory_size);

							if mapping.as_ptr_range().end.addr() > highest_addr {
								highest_addr = mapping.as_ptr_range().end.addr();
							}

							(mapping_key, mapping.virtual_valid_start())  // fixme: is it sound to drop the mapping here?
						};

						let res = match block_on(
							handle.kernel_syscall(
								<dyn Read>::UID,
								1,
								[
									page.as_ptr().addr() + segment_page_offset,
									segment.file_size.try_into().unwrap(),
									0,
									0,
								]
							)
						) {
							Ok(res) => res,
							e @ Err(Error::InvalidArg | Error::InvalidHandle | Error::InvalidPointer) => e.expect("args should be valid"),
							Err(e) => Err(e)?,
						};
						if (res as u64) < segment.file_size { Err(Error::EndOfData)?; }

						let mut mapping = address_space.get(mapping_key).expect("mapping should still exist");
						let zero_start = unsafe {
							let ptr = mapping.as_mut_ptr()
							                 .byte_add(segment.file_size.try_into().unwrap())
							                 .byte_add(segment_page_offset);
							kernel_api::ptr::slice_from_raw_parts_mut(
								ptr,
								(segment.memory_size - segment.file_size).try_into().unwrap(),
							)
						};
						zero_start.fill(0).unwrap();

						// todo: make read-only pages actually read-only
					}
				} else if segment.segment_type == SegmentType::INTERPRETER {
					let _ = block_on(
						handle.kernel_syscall(
							<dyn Seek>::UID,
							2,
							[
								segment.file_location().0.start,
								0,
								0,
								0,
							]
						)
					)?;

					let mut buffer = Vec::<u8>::with_capacity(segment.file_size as usize);

					let res = match block_on(
						handle.kernel_syscall(
							<dyn Read>::UID,
							1,
							[
								buffer.as_mut_ptr().addr(),
								segment.file_size.try_into().unwrap(),
								0,
								0,
							]
						)
					) {
						Ok(res) => res,
						e @ Err(Error::InvalidArg | Error::InvalidHandle | Error::InvalidPointer) => e.expect("args should be valid"),
						Err(e) => Err(e)?,
					};
					if (res as u64) < segment.file_size { Err(Error::EndOfData)?; }

					unsafe { buffer.set_len(segment.file_size as usize) };

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

				if interp_header.file_type != Type::Shared {
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
		if header.file_type != Type::Executable {
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

