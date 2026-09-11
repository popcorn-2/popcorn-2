use core::any::Any;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};
use unwinding::abi::UnwindReasonCode;
use unwinding::panic::catch_unwind as catch_unwind_impl;
use kernel_api::is_x86_feature_detected;
use kernel_api::sync::RwSpinlock;

static PANIC_COUNT: AtomicUsize = AtomicUsize::new(0);

pub struct SymbolMap {
	map: Option<NonNull<[u8]>>,
}

unsafe impl Send for SymbolMap {}
unsafe impl Sync for SymbolMap {}

impl From<Option<NonNull<[u8]>>> for SymbolMap {
	fn from(map: Option<NonNull<[u8]>>) -> Self {
		SymbolMap { map }
	}
}

pub static SYMBOL_MAP: RwSpinlock<SymbolMap> = RwSpinlock::new(SymbolMap { map: None });

pub fn catch_unwind<R, F: FnOnce() -> R + core::panic::UnwindSafe>(f: F) -> Result<R, Box<dyn Any + Send>> {
	let res = catch_unwind_impl(f);
	PANIC_COUNT.store(0, Ordering::Relaxed);
	res
}

pub struct Symbol {
	pub name: &'static str,
	pub file: &'static str,
}

pub fn get_symbol_from_ip(ip: usize) -> Symbol {
	#[cfg_attr(kasan, expect(unused))]
	struct SymbolMapIterator {
		index: usize,
		str: NonNull<[u8]>
	}

	impl Iterator for SymbolMapIterator {
		type Item = (usize, &'static str, &'static str);

		#[cfg(kasan)]
		fn next(&mut self) -> Option<Self::Item> {
			None // todo
		}

		#[cfg(not(kasan))]
		fn next(&mut self) -> Option<Self::Item> {
			let str = unsafe { self.str.as_ref() };

			let original_idx = self.index;
			if original_idx == str.len() { return None; }

			let mut idx = original_idx;
			while str[idx] != b'\n' { idx += 1; }

			let data = core::str::from_utf8(&str[original_idx..idx]).ok()?;
			let addr = &data[0..16];
			let (name, filename) = (&data[19..]).split_once('\t')?;
			let filename = match filename.split_once(':') {
				Some((filename, _)) => filename,
				None => filename
			};
			let addr = usize::from_str_radix(addr, 16).ok()?;

			self.index = idx + 1;

			Some((addr, name, filename))
		}
	}

	let Some(map) = SYMBOL_MAP.read().map else { return Symbol { name: "[no symbols]", file: "" }; };
	let iter = SymbolMapIterator {
		index: 0,
		str: map
	};
	let mut sym_name = "[unknown]";
	let mut sym_file = "[unknown]";
	for (sym_addr, name, file) in iter {
		if sym_addr > ip { break; }
		else if sym_addr != 0 { sym_name = name; sym_file = file; }
	}
	Symbol { name: sym_name, file: sym_file }
}

pub fn stack_trace_iter<F: FnMut(usize)>(mut f: F) {
	use unwinding::abi::{UnwindContext, _Unwind_GetIP, _Unwind_Backtrace};
	use core::ffi::c_void;

	extern "C" fn callback<F1: FnMut(usize)>(
		unwind_ctx: &UnwindContext<'_>,
		arg: *mut c_void,
	) -> UnwindReasonCode {
		let f = unsafe { &mut *arg.cast::<F1>() };
		let ip = _Unwind_GetIP(unwind_ctx);
		if ip != 0 {
			f(ip);
		}
		UnwindReasonCode::NO_REASON
	}

	if is_x86_feature_detected!("smap") { unsafe { core::arch::asm!("stac"); } }
	_Unwind_Backtrace(callback::<F>, ptr::addr_of_mut!(f).cast());
	if is_x86_feature_detected!("smap") { unsafe { core::arch::asm!("clac"); } }
}

pub fn stack_trace() {
	let mut counter = 0;
	stack_trace_iter(|ip| {
		let symbol = get_symbol_from_ip(ip - 1);
		counter += 1;
		sprintln!(
			"{:4}:{:#19x} - {} ({})",
			counter,
			ip - 1,
			symbol.name,
			symbol.file,
		);
	});
}

#[doc(hidden)]
pub fn stack_trace_fn(ptr: fn(usize, *const u8), ctx: *const u8) {
	stack_trace_iter(|ip| {
		ptr(ip, ctx);
	})
}

pub fn do_panic() -> ! {
	struct NoPayload;
	do_panic_with(Box::new(NoPayload))
}

pub fn do_panic_with(payload: Box<dyn Any + Send>) -> ! {
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

pub fn resume_unwind(payload: Box<dyn Any + Send>) -> ! {
	do_panic_with(payload)
}
