use core::fmt;
#[cfg(all(target_pointer_width = "64", target_has_atomic_load_store = "128"))]
use core::sync::atomic::AtomicU128;
#[cfg(all(target_pointer_width = "32", target_has_atomic_load_store = "64"))]
use core::sync::atomic::AtomicU64;
#[cfg(any(
	all(target_pointer_width = "64", target_has_atomic_load_store = "128"),
	all(target_pointer_width = "32", target_has_atomic_load_store = "64"),
))]
use core::sync::atomic::Ordering;

#[cfg(target_pointer_width = "64")]
type UfatInner = u128;
#[cfg(target_pointer_width = "32")]
type UfatInner = u64;

#[cfg(all(target_pointer_width = "64", target_has_atomic_load_store = "128"))]
type AtomicUfatInner = AtomicU128;
#[cfg(all(target_pointer_width = "32", target_has_atomic_load_store = "64"))]
type AtomicUfatInner = AtomicU64;

#[expect(non_camel_case_types, reason = "effective primitive type")]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
#[repr(transparent)]
pub struct ufat(UfatInner);

#[cfg(any(
	all(target_pointer_width = "64", target_has_atomic_load_store = "128"),
	all(target_pointer_width = "32", target_has_atomic_load_store = "64"),
))]
#[derive(Debug)]
#[repr(transparent)]
pub struct AtomicUfat(AtomicUfatInner);

impl ufat {
	pub const MIN: Self = Self(UfatInner::MIN);
	pub const MAX: Self = Self(UfatInner::MAX);
	pub const BITS: u32 = UfatInner::BITS;

	pub const fn new(upper: usize, lower: usize) -> Self {
		let upper = upper as UfatInner;
		let lower = lower as UfatInner;
		Self((upper << usize::BITS) | lower)
	}

	pub const fn upper(self) -> usize {
		(self.0 >> usize::BITS) as usize
	}

	pub const fn lower(self) -> usize {
		self.0 as usize
	}

	pub const fn truncate<T: const private::TruncateTarget<Self>>(self) -> T {
		T::truncate_from(self)
	}
}

impl fmt::Binary for ufat {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		<UfatInner as fmt::Binary>::fmt(&self.0, f)
	}
}

impl fmt::Display for ufat {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		<UfatInner as fmt::Display>::fmt(&self.0, f)
	}
}

impl fmt::LowerExp for ufat {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		<UfatInner as fmt::LowerExp>::fmt(&self.0, f)
	}
}

impl fmt::LowerHex for ufat {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		<UfatInner as fmt::LowerHex>::fmt(&self.0, f)
	}
}

impl fmt::Octal for ufat {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		<UfatInner as fmt::Octal>::fmt(&self.0, f)
	}
}

impl fmt::UpperExp for ufat {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		<UfatInner as fmt::UpperExp>::fmt(&self.0, f)
	}
}

impl fmt::UpperHex for ufat {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		<UfatInner as fmt::UpperHex>::fmt(&self.0, f)
	}
}

#[cfg(any(
	all(target_pointer_width = "64", target_has_atomic_load_store = "128"),
	all(target_pointer_width = "32", target_has_atomic_load_store = "64"),
))]
impl AtomicUfat {
	pub const fn new(v: ufat) -> Self { Self(AtomicUfatInner::new(v.0)) }
	pub fn load(&self, ordering: Ordering) -> ufat { ufat(self.0.load(ordering)) }
	pub fn store(&self, val: ufat, ordering: Ordering) { self.0.store(val.0, ordering); }
	pub fn swap(&self, val: ufat, order: Ordering) -> ufat { ufat(self.0.swap(val.0, order)) }
	pub fn compare_exchange(&self, current: ufat, new: ufat, success: Ordering, failure: Ordering) -> Result<ufat, ufat> {
		self.0.compare_exchange(current.0, new.0, success, failure)
			.map(ufat)
			.map_err(ufat)
	}
	pub fn fetch_update<F>(&self, set_order: Ordering, fetch_order: Ordering, mut f: F) -> Result<ufat, ufat>
	where
		F: FnMut(ufat) -> Option<ufat>
	{
		self.0.fetch_update(set_order, fetch_order, |val| f(ufat(val)).map(|val| val.0))
			.map(ufat)
			.map_err(ufat)
	}
}

mod private {
	pub const trait TruncateTarget<T> {
		fn truncate_from(val: T) -> Self;
	}
}

impl const private::TruncateTarget<ufat> for usize {
	fn truncate_from(val: ufat) -> Self { val.lower() }
}

impl const private::TruncateTarget<ufat> for u8 {
	fn truncate_from(val: ufat) -> Self { val.0.truncate() }
}

impl const private::TruncateTarget<ufat> for u16 {
	fn truncate_from(val: ufat) -> Self { val.0.truncate() }
}

impl const private::TruncateTarget<ufat> for u32 {
	fn truncate_from(val: ufat) -> Self { val.0.truncate() }
}
