use core::arch::asm;
use core::mem::MaybeUninit;

macro_rules! gen_checked {
    (@read $name:ident $ty:ty => $reg_constraint:tt $reg_modifier:tt) => {
		#[inline]
		pub fn $name (ptr: *const $ty) -> Option<$ty> {
			let ret: MaybeUninit<_>;
			let success: usize;
			// SAFETY:
			// - only register modifications are with `mov {}, X` using compiler allocated registers
			// - no external functions called so no unwinding can occur
			// - only memory pointed to by `ptr` is read
			// - no memory writes occur
			// - no use of stack so `options(nostack)` is sound
			// - x87 state not touched
			// - FLAGS state is not modified
			unsafe {
				if crate::detect::__detected::smap() {
					asm!(
						"stac",
						concat!("2: mov {", $reg_modifier, "}, [{}]"),
						"   mov {}, 1",
						".pushsection .popcorn.deref_handlers.check",
						".quad 2b",
						".popsection",
						".pushsection .popcorn.deref_handlers.handle",
						".quad 3f",
						".popsection",
						"3: ",
						"clac",
						inout($reg_constraint) MaybeUninit::<$ty>::uninit() => ret, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
						in(reg) ptr,
						inout(reg) 0usize => success,
						options(nostack, preserves_flags, readonly)
					);	
				} else {
					asm!(
						concat!("2: mov {", $reg_modifier, "}, [{}]"),
						"   mov {}, 1",
						".pushsection .popcorn.deref_handlers.check",
						".quad 2b",
						".popsection",
						".pushsection .popcorn.deref_handlers.handle",
						".quad 3f",
						".popsection",
						"3: ",
						inout($reg_constraint) MaybeUninit::<$ty>::uninit() => ret, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
						in(reg) ptr,
						inout(reg) 0usize => success,
						options(nostack, preserves_flags, readonly)
					);
				}
			}
		
			match success {
				0 => None,
				// SAFETY: if inline asm returns non-zero success, then `ret` has been initialized by the asm
				_ => Some(unsafe { ret.assume_init() }),
			}
		}
    };
	(@write $name:ident $ty:ty => $reg_constraint:tt $reg_modifier:tt) => {
		#[inline]
		pub fn $name(ptr: *mut $ty, val: $ty) -> Option<()> {
			let success: usize;
			// SAFETY:
			// - only register modified is with `mov {}, 1` using the compiler allocated register
			// - no external functions called so no unwinding can occur
			// - only memory pointed to by `ptr` is modified
			// - no use of stack so `options(nostack)` is sound
			// - x87 state not touched
			// - FLAGS state is not modified
			unsafe {
				if crate::detect::__detected::smap() {
					asm!(
						"stac",
						".pushsection .popcorn.deref_handlers.check",
						".quad 2f",
						".popsection",
						"2:",
						concat!("mov [{}], {", $reg_modifier, "}"),
						"mov {}, 1",
						".pushsection .popcorn.deref_handlers.handle",
						".quad 3f",
						".popsection",
						"3:",
						"clac",
						in(reg) ptr,
						in($reg_constraint) val,
						inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
						options(nostack, preserves_flags)
					);
				} else {
					asm!(
						".pushsection .popcorn.deref_handlers.check",
						".quad 2f",
						".popsection",
						"2:",
						concat!("mov [{}], {", $reg_modifier, "}"),
						"mov {}, 1",
						".pushsection .popcorn.deref_handlers.handle",
						".quad 3f",
						".popsection",
						"3:",
						in(reg) ptr,
						in($reg_constraint) val,
						inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
						options(nostack, preserves_flags)
					);
				}
			}
		
			match success {
				0 => None,
				_ => Some(()),
			}
		}	
	};
}

gen_checked!(@read checked_read_1 u8 => reg_byte "");
gen_checked!(@read checked_read_2 u16 => reg ":x");
gen_checked!(@read checked_read_4 u32 => reg ":e");
#[cfg(target_arch = "x86_64")] gen_checked!(@read checked_read_8 u64 => reg ":r");
gen_checked!(@write checked_write_1 u8 => reg_byte "");
gen_checked!(@write checked_write_2 u16 => reg ":x");
gen_checked!(@write checked_write_4 u32 => reg ":e");
#[cfg(target_arch = "x86_64")] gen_checked!(@write checked_write_8 u64 => reg ":r");

#[inline]
pub fn checked_memcpy(src: *const u8, dest: *mut u8, count: usize) -> Option<()> {
	let success: usize;
	// SAFETY:
	// - only register modified is with `mov {}, 1` using the compiler allocated register
	// - no external functions called so no unwinding can occur
	// - only memory pointed to by `dest` is modified
	// - only memory pointed to by `src` is read
	// - no use of stack so `options(nostack)` is sound
	// - DF not modified by asm
	// - x87 state not touched
	unsafe {
		if crate::detect::__detected::smap() {
			asm!(
				"stac",
				".pushsection .popcorn.deref_handlers.check",
				".quad 2f",
				".popsection",
				"2:",
				"rep movsb [rdi], [rsi]",
				"mov {}, 1",
				".pushsection .popcorn.deref_handlers.handle",
				".quad 3f",
				".popsection",
				"3:",
				"clac",
				inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
				in("rdi") dest,
				in("rsi") src,
				inout("rcx") count => _,
				options(nostack)
			);
		} else {
			asm!(
				".pushsection .popcorn.deref_handlers.check",
				".quad 2f",
				".popsection",
				"2:",
				"rep movsb [rdi], [rsi]",
				"mov {}, 1",
				".pushsection .popcorn.deref_handlers.handle",
				".quad 3f",
				".popsection",
				"3:",
				inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
				in("rdi") dest,
				in("rsi") src,
				inout("rcx") count => _,
				options(nostack)
			);
		}
	}

	match success {
		0 => None,
		_ => Some(()),
	}
}


#[inline]
pub fn checked_fill(val: u8, dest: *mut u8, count: usize) -> Option<()> {
	let success: usize;
	// SAFETY:
	// - only register modified is with `mov {}, 1` using the compiler allocated register
	// - no external functions called so no unwinding can occur
	// - only memory pointed to by `dest` is modified
	// - no use of stack so `options(nostack)` is sound
	// - DF not modified by asm
	// - x87 state not touched
	unsafe {
		if crate::detect::__detected::smap() {
			asm!(
			"stac",
			".pushsection .popcorn.deref_handlers.check",
			".quad 2f",
			".popsection",
			"2:",
			"rep stosb [rdi]",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 3f",
			".popsection",
			"3:",
			"clac",
			inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			in("rdi") dest,
			in("al") val,
			inout("rcx") count => _,
			options(nostack)
			);
		} else {
			asm!(
			".pushsection .popcorn.deref_handlers.check",
			".quad 2f",
			".popsection",
			"2:",
			"rep stosb [rdi]",
			"mov {}, 1",
			".pushsection .popcorn.deref_handlers.handle",
			".quad 3f",
			".popsection",
			"3:",
			inout(reg) 0usize => success, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
			in("rdi") dest,
			in("al") val,
			inout("rcx") count => _,
			options(nostack)
			);
		}
	}

	match success {
		0 => None,
		_ => Some(()),
	}
}
