#[allow(unused_imports)] use crate::prelude::*;
use core::any::Any;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::ops::Bound;
use core::iter;
use unwinding::abi::UnwindReasonCode;
use unwinding::panic::catch_unwind as catch_unwind_impl;
use kernel_api::sync::OnceLock;
use alloc::collections::btree_map::BTreeMap;

static PANIC_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static SYMBOL_MAP: OnceLock<&'static [u8]> = OnceLock::new();
static SYMBOL_TREE: OnceLock<BTreeMap<usize, Symbol>> = OnceLock::new();

pub fn catch_unwind<R, F: FnOnce() -> R + core::panic::UnwindSafe>(f: F) -> Result<R, Box<dyn Any + Send>> {
	let res = catch_unwind_impl(f);
	PANIC_COUNT.store(0, Ordering::Relaxed);
	res
}

#[derive(Copy, Clone)]
pub struct Symbol {
	pub name: &'static str,
	pub file: &'static str,
}

impl Symbol {
	const UNKNOWN: Symbol = Symbol { name: "[unknown]", file: "[unknown]" };
}

struct SymbolMapIterator {
	index: usize,
	str: &'static [u8]
}

impl Iterator for SymbolMapIterator {
	type Item = (usize, Symbol);

	fn next(&mut self) -> Option<Self::Item> {
		let original_idx = self.index;
		if original_idx == self.str.len() { return None; }

		let mut idx = original_idx;
		while self.str[idx] != b'\n' { idx += 1; }

		let data = core::str::from_utf8(&self.str[original_idx..idx]).ok()?;
		let addr = &data[0..16];
		let (name, file) = (&data[19..]).split_once('\t')?;
		let file = match file.split_once(':') {
			Some((file, _)) => file,
			None => file
		};
		let addr = usize::from_str_radix(addr, 16).ok()?;

		self.index = idx + 1;

		Some((addr, Symbol { name, file }))
	}
	
	fn size_hint(&self) -> (usize, Option<usize>) {
		let len = self.str.iter().filter(|c| **c == b'\n').count();
		(len, Some(len))
	}
}

pub struct EmptySymbolMapError;
pub fn construct_symbol_tree() -> Result<(), EmptySymbolMapError> {
	let Some(map) = SYMBOL_MAP.get() else { return Err(EmptySymbolMapError); };
	let iter = SymbolMapIterator {
		index: 0,
		str: map
	};
	SYMBOL_TREE.get_or_init(|| BTreeMap::from_iter(iter::once((0, Symbol::UNKNOWN)).chain(iter)));
	Ok(())
}

pub fn get_symbol_from_ip(ip: usize) -> Symbol {
	if let Some(symbol_tree) = SYMBOL_TREE.get() {
		return *symbol_tree.upper_bound(Bound::Included(&ip)).peek_prev()
			.expect("upper_bound returns a Cursor pointing to the gap after an element, so prev cannot be None unless the tree is empty").1;
	}

	let Some(map) = SYMBOL_MAP.get() else { return Symbol { name: "[no symbols]", file: "" }; };
	let iter = SymbolMapIterator {
		index: 0,
		str: map
	};
	
	let mut last_sym = Symbol::UNKNOWN;
	for (sym_addr, sym) in iter {
		if sym_addr > ip { break; }
		last_sym = sym;
	}
	return last_sym;
}

pub fn stack_trace_iter<F: FnMut(usize, Symbol)>(mut f: F) {
	use unwinding::abi::{UnwindContext, _Unwind_GetIP, _Unwind_Backtrace};
	use core::ffi::c_void;

	extern "C" fn callback<F1: FnMut(usize, Symbol)>(
		unwind_ctx: &UnwindContext<'_>,
		arg: *mut c_void,
	) -> UnwindReasonCode {
		let f = unsafe { &mut *arg.cast::<F1>() };
		let ip = _Unwind_GetIP(unwind_ctx);
		if ip != 0 {
			let symbol = get_symbol_from_ip(ip);
			f(ip, symbol);
		}
		UnwindReasonCode::NO_REASON
	}

	_Unwind_Backtrace(callback::<F>, ptr::addr_of_mut!(f).cast());
}

pub fn stack_trace() {
	let mut counter = 0;
	stack_trace_iter(|ip, name| {
		counter += 1;
		sprintln!(
			"{:4}:{:#19x} - {}",
			counter,
			ip,
			name.name,
		);
	});
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
