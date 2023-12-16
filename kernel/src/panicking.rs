use alloc::boxed::Box;
use core::any::Any;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use unwinding::abi::UnwindReasonCode;
use unwinding::panic::catch_unwind as catch_unwind_impl;
use kernel_api::sync::RwLock;
use crate::sprintln;

static PANIC_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static SYMBOL_MAP: RwLock<Option<&'static [u8]>> = RwLock::new(None);

pub fn catch_unwind<R, F: FnOnce() -> R>(f: F) -> Result<R, Box<dyn Any + Send>> {
	let res = catch_unwind_impl(f);
	PANIC_COUNT.store(0, Ordering::Relaxed);
	res
}

fn get_symbol_name(ip: usize) -> &'static str {
	struct SymbolMapIterator {
		index: usize,
		str: &'static [u8]
	}

	impl Iterator for SymbolMapIterator {
		type Item = (usize, &'static str);

		fn next(&mut self) -> Option<Self::Item> {
			let original_idx = self.index;
			if original_idx == self.str.len() { return None; }

			let mut idx = original_idx;
			while self.str[idx] != b'\n' { idx += 1; }

			let data = core::str::from_utf8(&self.str[original_idx..idx]).ok()?;
			let addr = &data[0..16];
			let name = &data[19..];
			let addr = usize::from_str_radix(addr, 16).ok()?;

			self.index = idx + 1;

			Some((addr, name))
		}
	}

	let Some(map) = *SYMBOL_MAP.read() else { return "<no symbols>"; };
	let iter = SymbolMapIterator {
		index: 0,
		str: map
	};
	let mut sym_name = "<unknown>";
	for (sym_addr, name) in iter {
		if sym_addr > ip { break; }
		else if sym_addr != 0 { sym_name = name; }
	}
	sym_name
}

pub fn stack_trace() {
	use unwinding::abi::{UnwindContext, _Unwind_GetIP, _Unwind_Backtrace};
	use core::ffi::c_void;

	struct CallbackData {
		counter: usize,
	}
	extern "C" fn callback(
		unwind_ctx: &UnwindContext<'_>,
		arg: *mut c_void,
	) -> UnwindReasonCode {
		let data = unsafe { &mut *arg.cast::<CallbackData>() };
		data.counter += 1;
		let ip = _Unwind_GetIP(unwind_ctx);
		sprintln!(
			"{:4}:{:#19x} - {}",
			data.counter,
			ip,
			get_symbol_name(ip)
		);
		UnwindReasonCode::NO_REASON
	}
	let mut data = CallbackData { counter: 0 };
	_Unwind_Backtrace(callback, ptr::addr_of_mut!(data).cast());
}

pub(crate) fn do_panic() -> ! {
	struct NoPayload;
	do_panic_with(Box::new(NoPayload))
}

fn do_panic_with(payload: Box<dyn Any + Send>) -> ! {
	#[cfg(panic = "unwind")]
	{
		#[cfg(not(test))]
		stack_trace();

		if PANIC_COUNT.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed).is_err() {
			// PANIC_COUNT not at 0
			// already unwinding
			sprintln!("\u{001b}[31m\u{001b}[1mFATAL: kernel panicked while processing panic.\u{001b}[0m");
			loop {}
		} else {
			// new unwind
			let code = unwinding::panic::begin_panic(payload);
			if code == UnwindReasonCode::END_OF_STACK {
				sprintln!("\u{001b}[31m\u{001b}[1mFATAL: aborting\u{001b}[0m");
			} else {
				sprintln!("\u{001b}[31m\u{001b}[1mFATAL: failed to panic, error {}\u{001b}[0m", code.0);
			}
			loop {}
		}
	}

	#[cfg(not(panic = "unwind"))]
	loop {}
}

pub(crate) fn panicking() -> bool {
	PANIC_COUNT.load(Ordering::Relaxed) >= 1
}

pub(crate) fn resume_unwind(payload: Box<dyn Any + Send>) -> ! {
	do_panic_with(payload)
}
