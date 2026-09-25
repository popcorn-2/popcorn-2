//! Provides pointer wrappers for safely accessing userspace.
//!
//! # User pointers
//!
//! TODO(doc).
//!
//! ## Local pointers
//!
//! TODO(doc).
//!
//! All pointers provided from userspace should be accessed through a [`User`] to
//! prevent kernelspace page faults from invalid or misaligned pointers.
//! 
//! # SMAP
//! 
//! When SMAP is supported on the system, all memory access through a [`User`] will
//! automatically set and clear the `AC` flag to prevent trapping.

#[cfg(feature = "full")]
mod user_ptr;

use core::fmt;
use core::fmt::Formatter;
use core::num::NonZero;
use core::ptr::NonNull;
#[cfg(feature = "full")]
pub use user_ptr::*;

#[cfg(feature = "full")]
mod user_local;
#[cfg(feature = "full")]
pub use user_local::*;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[path = "x86_64.rs"]
#[cfg(feature = "full")]
mod impls;

/// The error returned when a memory access to userspace failed.
#[derive(Debug)]
#[non_exhaustive]
pub struct PointerError {}

/// A [`NonNull<T>`] pointer with a numerical tag.
///
/// The tag is split between the low alignment bits of the inner type, and the high
/// order bits that are free due to the architecture virtual address width.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct TaggedNonNull<T: ?Sized>(NonNull<T>);

impl<T> TaggedNonNull<T> {
	pub const TAG_HIGH_BITS: u32 = cfg_select! {
		// FIXME(LA57)
		target_arch = "x86_64" => 16,
	};
	pub const TAG_LOW_BITS: u32 = align_of::<T>().trailing_zeros();

	const MAX_BOUND_VALUE: usize = {
		assert!(
			Self::TAG_HIGH_BITS + Self::TAG_LOW_BITS < usize::BITS,
			"cannot have pointer which is only tag",
		);

		1 << (Self::TAG_HIGH_BITS + Self::TAG_LOW_BITS)
	};

	/// The (architecture- and type-dependent) maximum value the tag can hold.
	pub const MAXIMUM_TAG: usize = Self::MAX_BOUND_VALUE - 1;

	const TAG_HIGH_MASK: usize = ((1 << Self::TAG_HIGH_BITS) - 1) << (usize::BITS - Self::TAG_HIGH_BITS);
	const TAG_LOW_MASK: usize = (1 << Self::TAG_LOW_BITS) - 1;

	/// Create a new `TaggedNonNull` with the given `tag`.
	pub fn new(ptr: NonNull<T>, tag: usize) -> Self {
		assert!(tag <= Self::MAX_BOUND_VALUE, "requested tag cannot fit in pointer");
		assert!(ptr.is_aligned(), "cannot tag unaligned pointer");
		let tag_low = tag & Self::TAG_LOW_MASK;
		let tag_high = tag >> Self::TAG_LOW_BITS;
		let ptr = ptr.map_addr(|mut addr| {
			// SAFETY: assertion that non-tag bits exist in the pointer, and original pointer came from NonNull
			addr = unsafe { NonZero::new_unchecked(addr.get() & !(Self::TAG_HIGH_MASK | Self::TAG_LOW_MASK)) };
			addr |= tag_low;
			addr |= tag_high << (usize::BITS - Self::TAG_HIGH_BITS);
			addr
		});
		Self(ptr)
	}

	/// Extract the pointer portion of the `TaggedNonNull`.
	pub fn as_ptr(&self) -> NonNull<T> {
		let ptr = self.0.as_ptr();
		let ptr = ptr.map_addr(|addr| {
			let masked = addr & !(Self::TAG_HIGH_MASK | Self::TAG_LOW_MASK);
			let canonical = (masked << Self::TAG_HIGH_BITS).cast_signed() >> Self::TAG_HIGH_BITS;
			canonical.cast_unsigned()
		});
		// SAFETY: assertion that non-tag bits exist in the pointer, and original pointer came from NonNull
		unsafe { NonNull::new_unchecked(ptr) }
	}

	/// Extract the tag portion of the `TaggedNonNull`.
	pub fn tag(&self) -> usize {
		let addr = self.0.addr().get();
		let low = addr & Self::TAG_LOW_MASK;
		let high = addr & Self::TAG_HIGH_MASK;
		low | ((high >> (usize::BITS - Self::TAG_HIGH_BITS)) << Self::TAG_LOW_BITS)
	}

	/// Compares the *addresses* of the two pointers for equality,
	/// ignoring the tag value.
	pub fn addr_eq(&self, other: &Self) -> bool {
		core::ptr::addr_eq(self.as_ptr().as_ptr(), other.as_ptr().as_ptr())
	}
}

impl<T> fmt::Pointer for TaggedNonNull<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		let addr = self.as_ptr().addr().get();
		let meta = core::ptr::metadata(self.as_ptr().as_ptr());
		let tag = self.tag();

		// copied from `impl fmt::Pointer for *const T`
		let mut ptr_options = f.options();
		if f.options().get_alternate() {
			ptr_options.sign_aware_zero_pad(true);

			if f.options().get_width().is_none() {
				ptr_options.width(Some((usize::BITS / 4) as u16 + 2));
			}
		}

		f.debug_struct("TaggedNonNull")
			.field_with("addr", move |f| fmt::LowerHex::fmt(&addr, &mut f.with_options(ptr_options)))
			.field("metadata", &meta)
			.field("tag", &tag)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tagged_nonnull_round_trip() {
		let x = &5;
		let tag = 0xCAFEusize;
		let ptr = TaggedNonNull::new(
			NonNull::from_ref(x),
			tag,
		);
		assert_eq!(ptr.tag(), tag);
		assert_eq!(ptr.as_ptr(), NonNull::from_ref(x));
	}
}
