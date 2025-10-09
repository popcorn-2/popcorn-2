use core::arch::asm;
use core::hint::unreachable_unchecked;
use core::mem::MaybeUninit;

macro_rules! gen_checked {
    (@read $name:ident $ty:ty => $reg_constraint:tt $reg_modifier:tt) => {
		#[inline]
		pub fn $name (ptr: *const MaybeUninit<$ty>) -> Option<MaybeUninit<$ty>> {
			let r: MaybeUninit<_>;
			let success: usize;
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
						inout($reg_constraint) MaybeUninit::<$ty>::uninit() => r, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
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
						inout($reg_constraint) MaybeUninit::<$ty>::uninit() => r, // we need to use `inout` here to ensure the initial value is what we want if the `mov` is never reached
						in(reg) ptr,
						inout(reg) 0usize => success,
						options(nostack, preserves_flags, readonly)
					);
				}
			}
		
			match success {
				0 => None,
				1 => Some(r),
				_ => unsafe { unreachable_unchecked() }
			}
		}
    };
	(@write $name:ident $ty:ty => $reg_constraint:tt $reg_modifier:tt) => {
		#[inline]
		pub fn $name(ptr: *mut MaybeUninit<$ty>, val: MaybeUninit<$ty>) -> Option<()> {
			let success: usize;
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
						options(nostack, preserves_flags, readonly)
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
						options(nostack, preserves_flags, readonly)
					);
				}
			}
		
			match success {
				0 => None,
				1 => Some(()),
				_ => unsafe { unreachable_unchecked() }
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
pub fn checked_memcpy(src: *const MaybeUninit<u8>, dest: *mut MaybeUninit<u8>, count: usize) -> Option<()> {
	let success: usize;
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
		1 => Some(()),
		_ => unsafe { unreachable_unchecked() }
	}
}


#[inline]
pub fn checked_fill(val: MaybeUninit<u8>, dest: *mut MaybeUninit<u8>, count: usize) -> Option<()> {
	let success: usize;
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
		1 => Some(()),
		_ => unsafe { unreachable_unchecked() }
	}
}
