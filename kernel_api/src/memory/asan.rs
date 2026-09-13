//! ABI for interfacing with KASAN shadow memory.
//!
//! TODO(doc): overview of kasan?
//!
//! KASAN functions by keeping an in-memory shadow map of all kernelspace memory.
//! Each byte in the shadow map represents 8 bytes of kernel memory, which can either be
//! accessible, partially accessible, or poisoned.
//!
//! There are a range of poison values used in Popcorn2, some of which are defined by compiler ABI.
//! In practice not all of these values are used.
//! The current list is:
//! - `0xfa`: Heap left redzone - memory just before a heap allocation
//! - `0xfb`: Heap right redzone - memory just after a heap allocation
//! - `0xfc`: Heap headers - memory used by heap internals
//! - `0xfd`: Freed heap memory - heap memory that has recently been deallocated, and is currently
//!   in a quarantine period
//! - `0xf1`: Stack left redzone - memory just before a stack allocation
//! - `0xf2`: Stack mid redzone - memory between two stack allocations
//! - `0xf3`: Stack right redzone - memory just after a stack allocation
//! - `0xf4`: Stack guard page - the page of unmapped memory below the stack to catch stack overflows
//! - `0xf5`: Stack after return - the stack frame of a function that has already returned, used to
//!   catch dangling references returned by a function
//! - `0xf8`: Stack use after scope - a stack slot in the current function but is now out of scope
//! - `0xf9`: Global redzone - memory around global variables
//! - `0xc0`: Freed virtual memory - memory that has just been deallocated by a [`Vmm`](crate::allocator::Vmm)
//! - `0xc1`: Uninitialized virtual memory - memory that has never been allocated
//! - `0xcc`: Shadow gap - the shadow map itself
//!
//! The entire region from [`SHADOW_MAP_START`] through to [`SHADOW_MAP_END`] is safe to read and write
//! to as a `[u8]`, since any unmapped memory will be automatically mapped. Helper functions for this are
//! provided as [`read_shadow_map_for`]/[`write_shadow_map_for`] and [`read_shadow_map_raw`]/[`write_shadow_map_raw`].

use crate::memory::VirtualAddress;

/// Offset to add to `addr / 8` to calculate shadow map address.
pub const SHADOW_MAP_SHIFT: usize = cfg_select! {
	target_arch = "x86_64" => 0xdfff_d000_0000_0000,
};

/// Address of the start of the shadow map region.
pub const SHADOW_MAP_START: VirtualAddress = cfg_select! {
	target_arch = "x86_64" => VirtualAddress::new(0xffff_c000_0000_0000),
};

/// Address of the end of the shadow map region.
pub const SHADOW_MAP_END: VirtualAddress = cfg_select! {
	target_arch = "x86_64" => SHADOW_MAP_START + SHADOW_MAP_SIZE,
};

const SHADOW_MAP_SIZE: usize = cfg_select! {
	target_arch = "x86_64" => 16*1024*1024*1024*1024,
};

/// Converts the passed `address` into the corresponding address in the shadow map.
#[must_use]
pub fn mem_to_shadow(address: VirtualAddress) -> VirtualAddress {
	let ret = (address.addr >> 3) + SHADOW_MAP_SHIFT;
	let ret = VirtualAddress::new(ret);
	debug_assert!(ret >= SHADOW_MAP_START && ret < SHADOW_MAP_END, "address = {address:#x}, ret = {ret:#x}, start = {SHADOW_MAP_START:#x}, end = {SHADOW_MAP_END:#x}");
	ret
}

/// Converts the number of bytes into the lower bound number of bytes in the shadow map.
#[must_use]
pub const fn count_to_shadow(count: usize) -> usize {
	count / 8
}

#[cfg(feature = "full")] pub use full::*;
#[cfg(feature = "full")] mod full {
	pub use super::*;
	use core::cmp::max;
	use crate::memory::VirtualAddress;
	use core::fmt::Write as _;
	use log::{debug, warn};

	const BYTES_AROUND: usize = 32*8*4;

	const LEGEND: &str = "\
Shadow byte legend (one shadow byte represents 8 kernel bytes):
  Addressable:           00
  Partially addressable: 01 - 07
  Heap left redzone:     fa
  Heap right redzone:    fb
  Heap headers:          fc
  Freed heap memory:     fd
  Stack left redzone:    f1
  Stack mid redzone:     f2
  Stack right redzone:   f3
  Stack guard page:      f4
  Stack after return:    f5
  Use after scope:       f8
  Global redzone:        f9
  Freed vmem:            c0
  Uninitialised vmem:    c1
  Shadow gap:            cc
";

    /// Prevents the contained code from being checked with the address sanitizer.
	///
	/// This should be used when accessing memory known to be marked as poisoned, for example
	/// inside a heap allocator when reading internal metadata contained in a _heap headers_ zone.
	/// This does **not** need to be used to wrap any functions in this module.
	///
	/// This macro should be called as a wrapper around closure-like syntax, either as
	/// `no_asan_shim!(|a: usize| -> usize { a + 1 })` or `no_asan_shim!(|a: usize| a)`.
	/// Generic arguments can be specified in square brackets before the closure, for example
	/// `no_asan_shim!([T: Add<usize>]|a: T| -> T::Output { a + 1 })`.
	/// All types including return types must be specified.
	/// Arguments must correspond to existing in-scope bindings.
	///
	/// # Examples
	///
	/// ```no_run
	/// use kernel_api::memory::asan::no_asan_shim;
	///
	/// for chunk in heap_chunks {
	///     let allocated = no_asan_shim!(|chunk: *mut Chunk| -> bool {
	///         unsafe { (&raw const (*chunk).allocated).read() }
	///     });
	///     if allocated { info!("allocated heap memory at {chunk:p}"); }
	/// }
	/// ```
	pub macro no_asan_shim {
		($([$($tt:tt)*])?|$($i:ident:$ty:ty),*$(,)?| $(-> $ret:ty)? $e:block) => {{
			#[cfg_attr(kasan, sanitize(address = "off"))]
			#[cfg_attr(kasan, inline(never))]
			#[cfg_attr(not(kasan), inline(always))]
			fn noasan_shim<$($tt)*>($($i:$ty),*) $(-> $ret)? {$e}

			noasan_shim($($i),*)
		}},
		($([$($tt:tt)*])?|$($i:ident:$ty:ty),*$(,)?| $e:expr) => {
			$crate::memory::asan::no_asan_shim!($([$($tt)*])?|$($i:$ty),*| { $e:expr })
		},
	}


	/// Reads the value of the shadow map at the index `idx`.
	///
	/// See the [module level documentation](`crate::memory::asan`) for information
	/// on the meaning of the read values.
	///
	/// # Panics
	///
	/// If `idx` is greater than `SHADOW_MAP_END - SHADOW_MAP_START`.
	#[must_use]
	pub fn read_shadow_map_raw(idx: usize) -> i8 {
		assert!(idx < SHADOW_MAP_SIZE, "attempt to read outside of shadow map");
		no_asan_shim!(|idx: usize| -> i8 {
			// SAFETY: just checked the index is within the shadow map and all values within shadow map
			//  are aligned and accessible due to lazy mapping
			unsafe { *SHADOW_MAP_START.as_ptr().byte_add(idx).cast() }
		})
	}

	/// Reads the value of the shadow map for `addr`.
	///
	/// See the [module level documentation](`crate::memory::asan`) for information
	/// on the meaning of the read values.
	///
	/// # Panics
	///
	/// If `addr` is not covered by the shadow map (i.e. userspace addresses).
	#[must_use]
	pub fn read_shadow_map_for(addr: VirtualAddress) -> i8 {
		read_shadow_map_raw((addr.addr >> 3) + SHADOW_MAP_SHIFT)
	}

	/// Write `val` to the shadow map at the index `idx`.
	///
	/// See the [module level documentation](`crate::memory::asan`) for information
	/// on the meaning of values that can be written.
	///
	/// # Panics
	///
	/// If `idx` is greater than `SHADOW_MAP_END - SHADOW_MAP_START`.
	pub fn write_shadow_map_raw(idx: usize, val: i8) {
		assert!(idx < SHADOW_MAP_SIZE, "attempt to read outside of shadow map");
		no_asan_shim!(|idx: usize, val: i8| {
			// SAFETY: just checked the index is within the shadow map and all values within shadow map
			//  are aligned and accessible due to lazy mapping
			unsafe { *SHADOW_MAP_START.as_ptr().byte_add(idx).cast() = val };
		});
	}

	/// Write `val` to the shadow map for `addr`.
	///
	/// See the [module level documentation](`crate::memory::asan`) for information
	/// on the meaning of values that can be written.
	///
	/// # Panics
	///
	/// If `addr` is not covered by the shadow map (i.e. userspace addresses).
	pub fn write_shadow_map_for(addr: VirtualAddress, val: i8) {
		write_shadow_map_raw((addr.addr >> 3) + SHADOW_MAP_SHIFT, val);
	}

	struct Serial;

	impl core::fmt::Write for Serial {
		fn write_str(&mut self, s: &str) -> core::fmt::Result {
			unsafe extern "Rust" {
				#[link_name = "__popcorn_force_unsafe_serial"]
				fn force_serial(s: &str);
			}
			// SAFETY: At worst this should cause interleaved access to the UART chip
			//  which would cause interleaved or corrupted serial output.
			//  At this point the kernel is already dying so just do the best we can.
			unsafe { force_serial(s) };
			Ok(())
		}
	}

	/// # Panics
	///
	/// Unconditionally.
	#[cold]
	fn do_report(address: VirtualAddress, ty: &str, width: usize) {
		let mut writer = Serial;

		let dump_start = mem_to_shadow(
			max(
				address.saturating_sub(BYTES_AROUND),
				VirtualAddress::new(0xffff_8000_0000_0000)
			)
		);
		let dump_end = mem_to_shadow(address.saturating_add(BYTES_AROUND - 1)); // make this an inclusive range so we can print up to usize::MAX

		let _ = writeln!(&mut writer, "===========================================================");
		let _ = writeln!(&mut writer, "KASAN violation detected:");
		let _ = writeln!(&mut writer, "  {ty} of {width} bytes at {address:#x}");
		let _ = writeln!(&mut writer, "Shadow bytes around the buggy address:");

		for i in dump_start..=dump_end {
			if (i - dump_start).is_multiple_of(16) {
				let _ = write!(&mut writer, "  0x{i:016x}:");
			}
			let byte = read_shadow_map_raw(i - SHADOW_MAP_START);

			if i >= mem_to_shadow(address) && i <= mem_to_shadow(address + width - 1usize) {
				let _ = write!(&mut writer, " \u{001b}[1m{byte:02x}\u{001b}[0m");
			} else {
				let _ = write!(&mut writer, " {byte:02x}");
			}

			if (i - dump_start).is_multiple_of(16) {
				let _ = writeln!(&mut writer);
			}
		}

		let _ = writeln!(&mut writer, "\n{LEGEND}");

		let _ = writeln!(&mut writer, "===========================================================");

		panic!("fatal KASAN error");
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	#[expect(clippy::missing_const_for_fn, reason = "currently unimplemented but would not be able to be const")]
	pub extern "C-unwind" fn __asan_register_globals(_globals: *const u8, _num: usize) {}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	#[expect(clippy::missing_const_for_fn, reason = "currently unimplemented but would not be able to be const")]
	pub extern "C-unwind" fn __asan_unregister_globals(_globals: *const u8, _num: usize) {}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_load1(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "load", 1);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_load2(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "load", 2);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_load4(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "load", 4);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_load8(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "load", 8);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_load16(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "load", 16);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_load_n(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		do_report(address, "load", count);
	}

	fn asan_mem_n(address: VirtualAddress, count: usize, ty: &str) {
		if !cfg!(kasan) { return; }

		let end = address + count;
		for byte in (address..end).step_by(8) {
			let shadow = read_shadow_map_for(byte);
			if shadow < 0 {
				do_report(address, ty, count);
			} else if shadow != 0 {
				let bytes_left = core::cmp::min(end - byte, 8);
				if bytes_left > shadow.cast_unsigned() as usize {
					do_report(address, ty, count);
				}
			}
		}
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_load_n(address: VirtualAddress, count: usize) {
		asan_mem_n(address, count, "load");
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_store_n(address: VirtualAddress, count: usize) {
		asan_mem_n(address, count, "store");
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_store1(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "store", 1);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_store2(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "store", 2);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_store4(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "store", 4);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_store8(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "store", 8);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_store16(address: VirtualAddress) {
		if !cfg!(kasan) { return; }

		do_report(address, "store", 16);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	pub extern "C-unwind" fn __asan_report_store_n(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		do_report(address, "store", count);
	}

	#[doc(hidden)]
	#[unsafe(no_mangle)]
	//#[no_sanitize(address)]
	pub extern "C-unwind" fn __asan_handle_no_return() {
		if !cfg!(kasan) { return; }

		/* idk what to do here */
		/*let rsp: usize;
		unsafe {
			core::arch::asm!("mov {}, rsp", out(reg) rsp);
		}
		let rsp_shadow = SHADOW_MAP_SHIFT + (rsp >> 3);
		let _ = writeln!(&mut Serial, "__asan_handle_no_return - clearing from {rsp_shadow:#x} to {SHADOW_MAP_END:#x}");
		for i in 0..(SHADOW_MAP_END as usize - rsp_shadow) {
			unsafe { *(rsp_shadow as *mut u8).offset(1) = 0; }
		}*/
		warn!("ignoring `__asan_handle_no_return`");
	}

	/// Marks `count` entries in the shadow map starting at `address` as _stack left redzone_.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[unsafe(export_name = "__asan_set_shadow_f1")]
	#[sanitize(address = "off")]
	#[inline(never)]
	#[cfg_attr(debug_assertions, expect(clippy::missing_panics_doc, reason = "panic is not guaranteed and only to check safety requirements"))]
	pub unsafe extern "C-unwind" fn set_shadow_stack_left(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		#[cfg(debug_assertions)] assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0xf1, count);
		}
	}

	/// Marks `count` entries in the shadow map starting at `address` as _stack use after scope_.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[unsafe(export_name = "__asan_set_shadow_f8")]
	#[sanitize(address = "off")]
	#[inline(never)]
	#[cfg_attr(debug_assertions, expect(clippy::missing_panics_doc, reason = "panic is not guaranteed and only to check safety requirements"))]
	pub unsafe extern "C-unwind" fn set_shadow_use_after_scope(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		#[cfg(debug_assertions)] assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0xf8, count);
		}
	}

	/// Marks `count` entries in the shadow map starting at `address` as _accessible_.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[unsafe(export_name = "__asan_set_shadow_00")]
	#[sanitize(address = "off")]
	#[inline(never)]
	pub unsafe extern "C-unwind" fn set_shadow_free(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		debug_assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0, count);
		}
	}

	/// Marks `count` entries in the shadow map starting at `address` as _freed virtual memory_.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[sanitize(address = "off")]
	#[inline(never)]
	pub unsafe extern "C-unwind" fn set_shadow_free_vmem(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		debug_assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0xc0, count);
		}
	}

	/// Marks `count` entries in the shadow map starting at `address` as _uninitialized virtual memory_.
	///
	/// This is the default value for lazily allocated shadow memory.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[sanitize(address = "off")]
	#[inline(never)]
	pub unsafe extern "C-unwind" fn set_shadow_uninit_vmem(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		debug_assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0xc1, count);
		}
	}

	/// Marks `count` entries in the shadow map starting at `address` as _heap left redzone_.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[unsafe(export_name = "__asan_set_shadow_fa")]
	#[sanitize(address = "off")]
	#[inline(never)]
	pub unsafe extern "C-unwind" fn set_shadow_heap_left(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		debug_assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0xfa, count);
		}
	}

	/// Marks `count` entries in the shadow map starting at `address` as _heap right redzone_.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[sanitize(address = "off")]
	#[inline(never)]
	pub unsafe extern "C-unwind" fn set_shadow_heap_right(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		debug_assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0xfb, count);
		}
	}

	/// Marks `count` entries in the shadow map starting at `address` as _heap headers_.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[sanitize(address = "off")]
	#[inline(never)]
	pub unsafe extern "C-unwind" fn set_shadow_heap_header(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		debug_assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0xfc, count);
		}
	}

	/// Marks `count` entries in the shadow map starting at `address` as _freed heap memory_.
	///
	/// # Safety
	///
	/// `address` through `address + count` must be addresses in the shadow map.
	#[unsafe(export_name = "__asan_set_shadow_fd")]
	#[sanitize(address = "off")]
	#[inline(never)]
	pub unsafe extern "C-unwind" fn set_shadow_heap_free(address: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		debug_assert!(address >= SHADOW_MAP_START && address < SHADOW_MAP_END, "Safety violation: {address:#x} is not in the shadow map");
		// SAFETY: `address` through `address + count` are addresses within the shadow map
		unsafe {
			core::ptr::write_bytes(address.as_ptr(), 0xfd, count);
		}
	}

	/// Marks the region from `start` to `start + count` as _accessible_.
	///
	/// This internally calculates the correct shadow map entries.
	///
	/// # Panics
	///
	/// If the region is not in the kernel region of the address space, or does not
	/// start on an 8 byte boundary.
	pub fn asan_free_range(start: VirtualAddress, count: usize) {
		if !cfg!(kasan) { return; }

		assert!(start.is_aligned_to(8), "asan free range must be 8 byte aligned");
		assert!(start.is_higher_half(), "asan only covers higher half");

		debug!("zero shadow memory ({:#x} -> {:#x})", mem_to_shadow(start), mem_to_shadow(start) + count_to_shadow(count));
		// SAFETY: checked that the address is in kernelspace, and `mem_to_shadow` returns
		//  a valid shadow map address for all kernelspace addresses.
		unsafe {
			set_shadow_free(
				mem_to_shadow(start),
				count_to_shadow(count),
			);
		};

		let last = start + count - 1usize;

		match count % 8 {
			0 => {},
			1 => write_shadow_map_for(last, 1),
			2 => write_shadow_map_for(last, 2),
			3 => write_shadow_map_for(last, 3),
			4 => write_shadow_map_for(last, 4),
			5 => write_shadow_map_for(last, 5),
			6 => write_shadow_map_for(last, 6),
			7 => write_shadow_map_for(last, 7),
			_ => unreachable!("x % 8 < 8"),
		}
	}
}
