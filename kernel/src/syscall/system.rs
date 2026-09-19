use kernel_api::ptr::TaggedNonNull;
use kernel_api::syscall;
use kernel_api::threading::TaskRef;
use crate::ebr;
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
) -> syscall::Result<ExitParams> {
	match koid.tag() & TAG_MASK {
		TAG_ADDRESS_SPACE => {
			todo!()
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
