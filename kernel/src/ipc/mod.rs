use alloc::sync::Arc;
use kernel_api::syscall;
use kernel_api::syscall::Handle;

#[doc(hidden)]
pub mod executor;

pub fn kernel_syscall_blocking(
	_this: &Arc<Handle>,
	_protocol: u128,
	_method: u32,
	_args: [usize; 4]
) -> syscall::Result<u128> {
	Err(syscall::Error::FutureCompat)
}

#[unsafe(no_mangle)]
fn __popcorn_handle_drop(this: &mut Handle) {
	debug!("drop handle {this:#x?}");
}
