use alloc::sync::Arc;
use core::bstr::ByteStr;
use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::ops::Deref;
use core::ptr::NonNull;
use core::slice;
use core::sync::atomic::Ordering;
use bitflags::{bitflags, Flags};
use kernel_api::address_space::AddressSpace;
use kernel_api::mapping::{Config, Mmap, Ty};
use kernel_api::memory::{VirtualAddress, PAGE_SIZE};
use kernel_api::num::ufat;
use kernel_api::ptr::TaggedNonNull;
use kernel_api::syscall;
use kernel_api::syscall::Handle;
use kernel_api::threading::{TaskRef, ThreadState};
use crate::{ebr, percpu, task};
use crate::memory::r#virtual::{AddressSpaceExt, AddressSpaceInner};
use crate::syscall::EntryParams;
use crate::task::{Task, TaskRefExt};

const TAG_ADDRESS_SPACE: usize = 0b001;
const TAG_TASK: usize = 0b010;
const TAG_CONSOLE: usize = 0b011;
const TAG_MASK: usize = 0b111;

fn get_oob_args<'a>(caller: &Task, params: &EntryParams, count: usize) -> syscall::Result<[&'a [u8]; 2]> {
	let mut offset = 0usize;
	let mut idx = 0;
	let map_arg = |arg: &usize| {
		if idx >= count { return Ok(&[] as &[u8]); }
		idx += 1;

		if offset.saturating_add(*arg) > PAGE_SIZE { return Err(syscall::Error::InvalidArg); }
		// SAFETY: checked won't exceed trampoline page size
		let start = unsafe { caller.syscall_trampoline_page().add(offset) };
		offset += *arg;
		// SAFETY: syscall trampoline page is always valid to read
		Ok(unsafe { slice::from_raw_parts(start, *arg) })
	};
	params.oob_args.each_ref().try_map(map_arg)
}

pub fn entry(
	ebr: &ebr::EpochGuard,
	koid: TaggedNonNull<u64>,
	caller: &'static Task,
	params: EntryParams,
) -> syscall::Result<ufat> {
	match (koid.tag() & TAG_MASK, params.interface) {
		(TAG_ADDRESS_SPACE, _) => {
			// SAFETY: AddressSpace koids own the contained AddressSpace, and the koid resides in a handle,
			//  so the AddressSpace will only be invalidated if the handle is dropped.
			//  We are holding an EBR guard, so the handle will not be dropped during the lifetime of
			//  this function.
			//  The AddressSpace gets wrapped in a ManuallyDrop, and so will not be cleaned up
			//  when it goes out of scope here.
			let address_space = unsafe { Arc::<AddressSpaceInner>::from_raw(koid.as_ptr().as_ptr().cast_const().cast()) };
			let address_space = ManuallyDrop::new(AddressSpace::__new(address_space));
			address_space_koid_entry(ebr, &*address_space, caller, params)
		},
		(TAG_TASK, _) => {
			let task_ref = unsafe { TaskRef::from_raw(koid) };
			task_koid_entry(ebr, task_ref, caller, params)
		},
		(TAG_CONSOLE, 1) => {
			match params.method {
				1 => {
					let oob_args = get_oob_args(caller, &params, 1)?;
					let payload = ByteStr::new(oob_args[0]);
					sprint!("{payload}");
					Ok(ufat::new(0, payload.len()))
				},
				_ => Err(syscall::Error::UnsupportedProtocol),
			}
		}
		(TAG_ADDRESS_SPACE..=TAG_CONSOLE, _) => Err(syscall::Error::UnsupportedProtocol),
		(tag, _) => unreachable!("invalid koid tag: {tag:#x}"),
	}
}

pub fn task_koid_entry(
	ebr: &ebr::EpochGuard,
	task_ref: TaskRef,
	caller: &'static Task,
	params: EntryParams,
) -> syscall::Result<ufat> {
	let check_current_task = || {
		if caller.as_ref() != task_ref {
			Err(syscall::Error::FutureCompat)
		} else { Ok(()) }
	};

	match (params.interface, params.method) {
		(2, 1) => {
			let exit_code = params.integer_args[0] as i32;
			task::kill_task(task_ref, exit_code)
				.map_err(|_| syscall::Error::DeadServer)?;
			Ok(ufat::new(0, 0))
		},
		(2, 2) => {
			check_current_task()?;
			let uaddr = params.integer_args[0];
			let expected = params.integer_args[1] as u32;
			task::futex_wait(caller, uaddr, expected)
		},
		(2, 3) => {
			check_current_task()?;
			let uaddr = params.integer_args[0];
			let count = params.integer_args[1];
			task::futex_wake(caller, uaddr, count)
		},
		(2, 4) => {
			check_current_task()?;

			bitflags! {
				struct CloneFlags: u32 {
					const SHARE_ADDRESS_SPACE = 1 << 0;
					const SHARE_HANDLES = 1 << 1;
				}
			}

			#[repr(C)]
			struct CloneInfo {
				address_space_handle: i32,

			}

			let address_space = clone_address_space(
				caller,
				(params.integer_args[0] as u32).cast_signed(),
				ebr,
			)?;

			/*let flags = CloneFlags::from_bits_truncate(params.integer_args[0] as u32);
			let address_space = if flags.contains(CloneFlags::SHARE_ADDRESS_SPACE) {
				AddressSpace::clone(&caller.address_space)
			} else {

			};*/

			let stack_ptr = params.integer_args[1];
			let entry_fn = params.integer_args[2];
			let info_ty = params.oob_args[0]; // FIXME: hacky
			let info_ptr = params.oob_args[1]; // FIXME: hacky

			let task = Task::alloc(address_space, |task| {
				let info_struct = task::ProcInfo::new_in(
					task,
					&[],
					vec![],
					info_ty,
					VirtualAddress::new(info_ptr),
					ebr
				).unwrap();

				task.registers.load_new_task(
					params.integer_args[1],
					params.integer_args[2],
					info_struct.addr,
				);
				task.state.store(ThreadState::Ready, Ordering::Relaxed);
				let _ = task.handles().swap(caller.handles().clone(ebr));
			})?;

			let handle = {
				let koid = TaskRef::new_koid(task.0.as_ref());
				let handle = Handle::new_system(koid);
				caller.handles().push(handle, ebr)?
			};

			percpu!(scheduler).enqueue(task);
			Ok(ufat::new(0, handle as usize))
		}
		(2, 5) => {
			task::wait_task(task_ref)
				.map(|exit_code| ufat::new(0, exit_code as usize))
		}
		(2, 6) => {
			check_current_task()?;
			percpu!(needs_reschedule).set(true);
			Ok(ufat::new(0, 0))
		}
		_ => Err(syscall::Error::UnsupportedProtocol),
	}
}

pub fn address_space_koid_entry(
	ebr: &ebr::EpochGuard,
	address_space: &AddressSpace,
	caller: &'static Task,
	params: EntryParams,
) -> syscall::Result<ufat> {
	match (params.interface, params.method) {
		(0, 1) => {
			let count = params.integer_args[0];
			let page_count = count.div_exact(PAGE_SIZE)
				.ok_or(syscall::Error::InvalidArg)?;
			let page_count = match NonZero::new(page_count) {
				Some(page_count) => page_count,
				None => return Ok(ufat::new(0, 0)),
			};

			let readable = params.integer_args[1] & 0b001 != 0;
			let executable = params.integer_args[1] & 0b010 != 0;
			let writeable = params.integer_args[1] & 0b100 != 0;

			let mapping = Config::new(page_count, Ty::USER_MMAP)
				.protection(writeable, executable, readable)
				.map_in::<Mmap>("".into(), address_space)?;
			Ok(ufat::new(0, mapping.mapping.virtual_valid_start().addr))
		},
		(0, 2) => {
			let ptr = params.integer_args[0];

			let mapping = address_space.get_by_addr(ptr)
				.ok_or(syscall::Error::InvalidArg)?;

			if mapping.virtual_valid_start().addr != ptr {
				Err(syscall::Error::FutureCompat)
			} else {
				mapping.remove();
				Ok(ufat::new(0, 0))
			}
		},
		(0, 3) => {
			bitflags! {
				struct VmCloneFlags: usize {
					/*
					/// Creates a copy of the VM space that inherits all memory but
					/// isolates future modifications.
					/// TODO: what happens to VMOs
					const FORK = 1 << 0;
					/// Returns [`Error::ConditionNotMet`] if no other references to
					/// the VM space exist.
					///
					/// This can be used to implement trusted processes, by forking
					/// their address space to prevent an untrusted parent from accessing
					/// their memory.
					const ONLY_IF_SHARED = 1 << 2;
					 */
				}
			}

			let flags = VmCloneFlags::from_bits_retain(params.integer_args[0]);

			if flags.contains_unknown_bits() {
				return Err(syscall::Error::FutureCompat);
			}

			let address_space = AddressSpace::empty()?;
			let koid = IntoKoid::new_koid(address_space);
			let handle = Handle::new_system(koid);
			caller.handles().push(handle, ebr)
				.map(|handle| ufat::new(0, handle as usize))
		},
		(0, 4) => {
			let target_ptr = params.integer_args[0];
			let data = get_oob_args(caller, &params, 1)?[0];
			todo!("find memory mapping corresponding to `target_ptr`, then copy into it from `data`")
		},
		_ => Err(syscall::Error::UnsupportedProtocol),
	}
}

fn clone_address_space(caller: &Task, handle: i32, ebr: &ebr::EpochGuard) -> syscall::Result<AddressSpace> {
	if handle == -4098 {
		Ok(caller.address_space.clone())
	} else {
		let handle = caller.handles().get(handle.cast_unsigned(), ebr)?;
		if !handle.is_system() { return Err(syscall::Error::InvalidArg); }
		let koid = handle.koid();
		if koid.tag() & TAG_MASK != TAG_ADDRESS_SPACE { return Err(syscall::Error::InvalidArg); }
		// SAFETY: See safety comment in `entry`
		let address_space = unsafe { Arc::<AddressSpaceInner>::from_raw(koid.as_ptr().as_ptr().cast_const().cast()) };
		let address_space = ManuallyDrop::new(AddressSpace::__new(address_space));
		Ok(address_space.deref().clone())
	}
}

pub trait IntoKoid: Sized {
	fn new_koid(this: Self) -> TaggedNonNull<u64>;
}

impl IntoKoid for TaskRef {
	fn new_koid(this: Self) -> TaggedNonNull<u64> {
		let mut tag = this.tag_raw();
		assert_eq!(tag & TAG_MASK, 0, "TaskRef should not contain lower tag bits");
		tag |= TAG_TASK;
		TaggedNonNull::new(
			this.as_ptr(),
			tag,
		)
	}
}

impl IntoKoid for AddressSpace {
	fn new_koid(this: AddressSpace) -> TaggedNonNull<u64> {
		let this = ManuallyDrop::new(this);
		let address_space = unsafe { Arc::from_raw(this.__extract_ptr(Ordering::SeqCst)) };
		let address_space = address_space.downcast::<AddressSpaceInner>().expect("`AddressSpace` should contain an `AddressSpaceInner`");
		TaggedNonNull::new(
			unsafe { NonNull::new_unchecked(Arc::into_raw(address_space).cast_mut()) }.cast(),
			TAG_ADDRESS_SPACE,
		)
	}
}

pub fn new_koid_serial() -> TaggedNonNull<u64> {
	TaggedNonNull::new(
		NonNull::dangling(),
		TAG_CONSOLE,
	)
}
