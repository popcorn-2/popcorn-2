use crate::time::Instant;

#[derive(Hash, Eq, PartialEq, Clone, Copy, Debug, Ord, PartialOrd)]
pub struct ServerId(usize, u64);

impl ServerId {
	#[doc(hidden)]
	pub const MAX: usize = u16::MAX as usize;

	/// A sentinel `ServerId` value which is guaranteed to never be assigned to a server
	pub const INVALID: ServerId = ServerId(usize::MAX, 0);

	#[doc(hidden)]
	pub fn new(val: u16) -> Self { Self(val.into(), Instant::now().get() as u64) } // assuming arch val is clock cycles, still takes >100 years to overflow generation number creating a new server every clock cycle at 5GHz
}
