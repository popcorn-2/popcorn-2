use kernel_api::ptr::LocalUser;
use kernel_api::{dbg, syscall};
use crate::ipc::Error;

pub async fn async_wait(buffer_ptr: usize, buffer_len: usize, min_return: usize) -> syscall::Result<u128> {
	if min_return > buffer_len {
		return Err(Error::InvalidArg);
	}

	let result_queue = {
		let guard = percpu_v2!(current_thread).read();
		let thread = guard
				.as_ref()
				.expect("cannot syscall from idle");
		super let map = thread.async_map.clone();
		&map.queue
	};

	let ptr = LocalUser::<*mut AsyncResult>::new(buffer_ptr);
	
	let mut completed = 0;
	
	let handle_packet = |key, result: syscall::Result<_>| {
		dbg!(AsyncResult {
			key,
			error: result.is_err(),
			value: result.unwrap_or_else(|e| e as u128),
		})
	};

	while completed < min_return && completed < buffer_len {
		let (key, packet) = result_queue.pop().await;
		let packet = handle_packet(key, packet);
		unsafe { ptr.add(completed) }.write(packet)?;
		completed += 1;
	}
	
	while completed < buffer_len && let Some((key, packet)) = result_queue.try_pop() {
		let packet = handle_packet(key, packet);
		unsafe { ptr.add(completed) }.write(packet)?;
		completed += 1;
	}
	
	Ok(completed as u128)
}

#[derive(Debug)]
#[repr(C)]
struct AsyncResult {
	key: usize,
	error: bool,
	value: u128,
}
