use kernel_api::dbg;
use crate::arch;
use kernel_api::syscall;

#[inline(always)]
pub unsafe fn entry(
	interface: usize,
	this: u32,
	flags: u16,
	method: u16,
	arg1: usize,
	arg2: usize,
	arg3: usize,
	arg4: usize,
	arg5: usize,
	stack_frame: arch::SyscallStackFrame,
) -> syscall::Result<(usize, u32, u16, u16, usize, usize, usize, usize, usize, arch::SyscallStackFrame)> {
	dbg!(
		interface,
		this,
		flags,
		method,
		arg1,
		arg2,
		arg3,
		arg4,
		arg5,
		stack_frame,
	);
	todo!()
}
