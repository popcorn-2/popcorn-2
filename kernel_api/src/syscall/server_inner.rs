use crate::time::Instant;

/// A unique identifier for a server.
#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug, Ord, PartialOrd)]
pub struct ServerId(usize, u64);

impl ServerId {
	#[doc(hidden)]
	pub const MAX: usize = u16::MAX as usize;

	/// A sentinel `ServerId` value which is guaranteed to never be assigned to a server.
	pub const INVALID: Self = Self(usize::MAX, 0);

	#[doc(hidden)]
	#[must_use]
	pub fn new(val: u16) -> Self { Self(val.into(), Instant::now().get().truncate::<u64>()) } // assuming arch val is clock cycles, still takes >100 years to overflow generation number creating a new server every clock cycle at 5GHz
}
