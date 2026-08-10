//! Provides extension traits for integer types.

/// Extension trait to truncate an integer to a smaller size.
///
/// If a value is larger than the target type, the most significant bits will be
/// ignored.
///
/// This is implemented for all uN/iN, truncating to strictly smaller sized integers.
/// Additionally, it is conditionally implemented for truncation to and from `usize`/`isize`
/// depending on target pointer size.
///
/// # Examples
///
/// ```
/// # use kernel_api::int_extension::Truncate;
///
/// let x: u32 = 257;
/// assert_eq!(x.truncate::<u16>(), 257u16);
/// assert_eq!(x.truncate::<u8>(), 1u8);
/// ```
///
/// ```ignore(only compiles on some targets)
/// # use kernel_api::int_extension::Truncate;
/// // this will only compile on targets where `target_ptr_width <= 64`
///
/// let x: u128 = 56;
/// assert_eq!(x.truncate::<usize>(), 56);
/// ```
pub const trait Truncate: Sized {
	/// Truncates `self` to type `T`.
	#[expect(rustdoc::missing_doc_code_examples, reason = "example in trait level docs")]
	fn truncate<T: const TruncateFrom<Self>>(self) -> T {
		T::truncate_from(self)
	}
}

#[doc(hidden)]
pub const trait TruncateFrom<T> {
	fn truncate_from(val: T) -> Self;
}

macro_rules! impl_truncate {
    ($($t:path)* => $u:path) => {
	    $(impl const TruncateFrom<$t> for $u {
		    fn truncate_from(val: $t) -> Self {
			    #![allow(clippy::cast_possible_truncation, reason = "definition of better function")]
			    val as $u
		    }
	    })*
    };
}

impl const Truncate for u128 {}
impl const Truncate for u64 {}
impl const Truncate for u32 {}
impl const Truncate for u16 {}

impl const Truncate for i128 {}
impl const Truncate for i64 {}
impl const Truncate for i32 {}
impl const Truncate for i16 {}

impl const Truncate for usize {}
impl const Truncate for isize {}

impl_truncate!(u128 u64 u32 u16 => u8);
impl_truncate!(u128 u64 u32 => u16);
impl_truncate!(u128 u64 => u32);
impl_truncate!(u128 => u64);

impl_truncate!(i128 i64 i32 i16 => i8);
impl_truncate!(i128 i64 i32 => i16);
impl_truncate!(i128 i64 => i32);
impl_truncate!(i128 => i64);

#[cfg(any(target_pointer_width = "16", target_pointer_width = "32", target_pointer_width = "64"))]
const _: () = {
	impl_truncate!(usize => u8);
	impl_truncate!(isize => i8);

	impl_truncate!(u128 => usize);
	impl_truncate!(i128 => isize);
};

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
const _: () = {
	impl_truncate!(usize => u16);
	impl_truncate!(isize => i16);
};

#[cfg(target_pointer_width = "64")]
const _: () = {
	impl_truncate!(usize => u32);
	impl_truncate!(isize => i32);
};

#[cfg(any(target_pointer_width = "32", target_pointer_width = "16"))]
const _: () = {
	impl_truncate!(u64 => usize);
	impl_truncate!(i64 => isize);
};

#[cfg(target_pointer_width = "16")]
const _: () = {
	impl_truncate!(u32 => usize);
	impl_truncate!(i32 => isize);
};
