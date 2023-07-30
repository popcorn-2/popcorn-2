use alloc::boxed::Box;
use core::any::Any;
use core::sync::atomic::{AtomicUsize, Ordering};
use unwinding::panic::catch_unwind as catch_unwind_impl;
use crate::sprintln;

static PANIC_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn catch_unwind<R, F: FnOnce() -> R>(f: F) -> Result<R, Box<dyn Any + Send>> {
	let res = catch_unwind_impl(f);
	PANIC_COUNT.store(0, Ordering::Relaxed);
	res
}

fn stack_trace() {
	use unwinding::abi::{UnwindContext, UnwindReasonCode, _Unwind_GetIP, _Unwind_Backtrace};
	use core::ffi::c_void;

	struct CallbackData {
		counter: usize,
	}
	extern "C" fn callback(
		unwind_ctx: &mut UnwindContext<'_>,
		arg: *mut c_void,
	) -> UnwindReasonCode {
		let data = unsafe { &mut *(arg as *mut CallbackData) };
		data.counter += 1;
		sprintln!(
			"{:4}:{:#19x} - <unknown>",
			data.counter,
			_Unwind_GetIP(unwind_ctx)
		);
		UnwindReasonCode::NO_REASON
	}
	let mut data = CallbackData { counter: 0 };
	_Unwind_Backtrace(callback, &mut data as *mut _ as _);
}

pub(crate) fn do_panic() -> ! {
	#[cfg(panic = "unwind")]
	{
		#[cfg(not(test))]
		stack_trace();

		if PANIC_COUNT.compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed).is_err() {
			// PANIC_COUNT not at 1
			// already unwinding
			sprintln!("FATAL: kernel panicked while processing panic.");
			loop {}
		} else {
			// new unwind
			struct NoPayload;
			let code = unwinding::panic::begin_panic(Box::new(NoPayload));
			sprintln!("FATAL: failed to panic, error {}", code.0);
			loop {}
		}
	}

	#[cfg(not(panic = "unwind"))]
	loop {}
}
