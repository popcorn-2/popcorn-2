use kernel_api::dbg;
use crate::arch;
use kernel_api::syscall;

#[derive(Debug)]
pub struct EntryParams {
	pub handle_num: u32,
	pub method: u16,
	pub interface: u64,
	pub integer_args: [usize; 3],
	pub oob_args: [usize; 2],
	pub stack_frame: arch::SyscallStackFrame,
}

#[derive(Debug)]
pub struct ExitParams {
	pub oid: u32,
	pub method: u16,
	pub interface: u64,
	pub integer_args: [usize; 3],
	pub oob_args: [usize; 2],
	pub stack_frame: arch::SyscallStackFrame,
}

#[inline(always)]
pub unsafe fn entry(params: EntryParams) -> syscall::Result<ExitParams> {
	debug!("enter syscall: {params:#x?}");
	todo!()
}
