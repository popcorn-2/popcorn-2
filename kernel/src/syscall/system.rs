use alloc::sync::Arc;
use core::mem::ManuallyDrop;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use kernel_api::address_space::AddressSpace;
use kernel_api::num::ufat;
use kernel_api::ptr::TaggedNonNull;
use kernel_api::syscall;
use kernel_api::threading::TaskRef;
use crate::ebr;
use crate::memory::r#virtual::AddressSpaceInner;
use crate::syscall::{EntryParams, ExitParams};
use crate::task::{Task, TaskRefExt};

const TAG_ADDRESS_SPACE: usize = 0b01;
const TAG_TASK: usize = 0b010;
const TAG_MASK: usize = 0b111;

pub fn entry(
	ebr: &ebr::EpochGuard,
	koid: TaggedNonNull<u64>,
	caller: &Task,
	params: EntryParams,
) -> syscall::Result<ufat> {
	match koid.tag() & TAG_MASK {
		TAG_ADDRESS_SPACE => {
			let address_space = unsafe { Arc::<AddressSpaceInner>::from_raw(koid.as_ptr().as_ptr().cast_const().cast()) };
			let address_space = ManuallyDrop::new(AddressSpace::__new(address_space));
			todo!("{address_space:?}")
		},
		TAG_TASK => {
			let task_ref = unsafe { TaskRef::from_raw(koid) };
			let task = task_ref.get().ok_or(syscall::Error::DeadServer)?;
			todo!("{:?}", task);
		},
		tag => unreachable!("invalid koid tag: {tag:#x}"),
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
