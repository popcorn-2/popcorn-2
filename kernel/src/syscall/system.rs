use alloc::sync::Arc;
use core::bstr::ByteStr;
use core::mem::ManuallyDrop;
use core::num::NonZero;
use core::ptr::NonNull;
use core::slice;
use core::sync::atomic::Ordering;
use kernel_api::address_space::AddressSpace;
use kernel_api::mapping::{Config, Mmap, Ty};
use kernel_api::memory::PAGE_SIZE;
use kernel_api::num::ufat;
use kernel_api::ptr::TaggedNonNull;
use kernel_api::syscall;
use kernel_api::threading::TaskRef;
use crate::{ebr, percpu};
use crate::syscall::{EntryParams, ExitParams};
use crate::memory::r#virtual::{AddressSpaceExt, AddressSpaceInner};
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
		(TAG_TASK, 2) => {
			let task_ref = unsafe { TaskRef::from_raw(koid) };
			if caller.as_ref() != task_ref {
				return Err(syscall::Error::FutureCompat);
			}
			match params.method {
				6 => {
					percpu!(needs_reschedule).set(true);
					Ok(ufat::new(0, 0))
				}
				_ => Err(syscall::Error::UnsupportedProtocol),
			}
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
		_ => Err(syscall::Error::UnsupportedProtocol),
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
