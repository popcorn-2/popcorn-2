use kernel_api::ptr::TaggedNonNull;
use kernel_api::syscall;
use crate::ebr;
use crate::syscall::{EntryParams, ExitParams};
use crate::task::Task;

pub fn entry(
	ebr: &ebr::EpochGuard,
	koid: TaggedNonNull<u8>,
	caller: &Task,
	params: EntryParams,
) -> syscall::Result<ExitParams> {
	todo!()
}
