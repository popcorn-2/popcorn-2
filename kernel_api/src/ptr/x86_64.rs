use core::arch::asm;
use core::hint::unreachable_unchecked;
use core::mem::MaybeUninit;

#[no_mangle]
#[inline]
pub unsafe fn checked_read_1(ptr: *const MaybeUninit<u8>) -> Option<MaybeUninit<u8>> {
	let r: MaybeUninit<_>;
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			"2: mov {}, [{}]",
			"   mov {}, 1",
			"3: ",
			// todo: "clac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 2b",
			".popsection",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 3b",
			".popsection",
			inout(reg_byte) MaybeUninit::<u8>::uninit() => r, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			in(reg) ptr,
			inout(reg) 0usize => success,
			options(nostack, preserves_flags, readonly)
		);
	}

	match success {
		0 => None,
		1 => Some(r),
		_ => unsafe { unreachable_unchecked() }
	}
}

#[inline]
pub unsafe fn checked_read_2(ptr: *const MaybeUninit<u16>) -> Option<MaybeUninit<u16>> {
	let r: MaybeUninit<_>;
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 1f",
			".popsection",
			"1:",
			"mov {:x}, [{}]",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 1f",
			".popsection",
			"1:",
			// todo: "clac",
			inout(reg) MaybeUninit::<u16>::uninit() => r, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			in(reg) ptr,
			inout(reg) 0usize => success,
			options(nostack, preserves_flags, readonly)
		);
	}

	match success {
		0 => None,
		1 => Some(r),
		_ => unsafe { unreachable_unchecked() }
	}
}

#[inline]
pub unsafe fn checked_read_4(ptr: *const MaybeUninit<u32>) -> Option<MaybeUninit<u32>> {
	let r: MaybeUninit<_>;
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 1f",
			".popsection",
			"1:",
			"mov {:e}, [{}]",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 1f",
			".popsection",
			"1:",
			// todo: "clac",
			inout(reg) MaybeUninit::<u32>::uninit() => r, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			in(reg) ptr,
			inout(reg) 0usize => success,
			options(nostack, preserves_flags, readonly)
		);
	}

	match success {
		0 => None,
		1 => Some(r),
		_ => unsafe { unreachable_unchecked() }
	}
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn checked_read_8(ptr: *const MaybeUninit<u64>) -> Option<MaybeUninit<u64>> {
	let r: MaybeUninit<_>;
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 1f",
			".popsection",
			"1:",
			"mov {:r}, [{}]",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 1f",
			".popsection",
			"1:",
			// todo: "clac",
			inout(reg) MaybeUninit::<u64>::uninit() => r, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			in(reg) ptr,
			inout(reg) 0usize => success,
			options(nostack, preserves_flags, readonly)
		);
	}

	match success {
		0 => None,
		1 => Some(r),
		_ => unsafe { unreachable_unchecked() }
	}
}

#[inline]
pub unsafe fn checked_write_1(ptr: *mut MaybeUninit<u8>, val: MaybeUninit<u8>) -> Option<()> {
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 1f",
			".popsection",
			"1:",
			"mov [{}], {}",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 1f",
			".popsection",
			"1:",
			// todo: "clac",
			in(reg) ptr,
			in(reg_byte) val,
			inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			options(nostack, preserves_flags, readonly)
		);
	}

	match success {
		0 => None,
		1 => Some(()),
		_ => unsafe { unreachable_unchecked() }
	}
}

#[inline]
pub unsafe fn checked_write_2(ptr: *mut MaybeUninit<u16>, val: MaybeUninit<u16>) -> Option<()> {
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 1f",
			".popsection",
			"1:",
			"mov [{}], {:x}",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 1f",
			".popsection",
			"1:",
			// todo: "clac",
			in(reg) ptr,
			in(reg) val,
			inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			options(nostack, preserves_flags, readonly)
		);
	}

	match success {
		0 => None,
		1 => Some(()),
		_ => unsafe { unreachable_unchecked() }
	}
}

#[inline]
pub unsafe fn checked_write_4(ptr: *mut MaybeUninit<u32>, val: MaybeUninit<u32>) -> Option<()> {
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 1f",
			".popsection",
			"1:",
			"mov [{}], {:e}",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 1f",
			".popsection",
			"1:",
			// todo: "clac",
			in(reg) ptr,
			in(reg) val,
			inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			options(nostack, preserves_flags, readonly)
		);
	}

	match success {
		0 => None,
		1 => Some(()),
		_ => unsafe { unreachable_unchecked() }
	}
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub unsafe fn checked_write_8(ptr: *mut MaybeUninit<u64>, val: MaybeUninit<u64>) -> Option<()> {
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 1f",
			".popsection",
			"1:",
			"mov [{}], {:r}",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 1f",
			".popsection",
			"1:",
			// todo: "clac",
			in(reg) ptr,
			in(reg) val,
			inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			options(nostack, preserves_flags, readonly)
		);
	}

	match success {
		0 => None,
		1 => Some(()),
		_ => unsafe { unreachable_unchecked() }
	}
}

#[inline]
pub unsafe fn checked_memcpy(src: *const MaybeUninit<u8>, dest: *mut MaybeUninit<u8>, count: usize) -> Option<()> {
	let success: usize;
	unsafe {
		asm!(
			// todo: "stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 1f",
			".popsection",
			"1:",
			"rep movsb [rdi], [rsi]",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 1f",
			".popsection",
			"1:",
			// todo: "clac",
			inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			in("rdi") dest,
			in("rsi") src,
			inout("rcx") count => _,
			options(nostack)
		);
	}

	match success {
		0 => None,
		1 => Some(()),
		_ => unsafe { unreachable_unchecked() }
	}
}
