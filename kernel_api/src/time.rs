#![unstable(feature = "kernel_time", issue = "none")]

use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

#[derive(Copy, Clone, Hash, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Instant {
	nanos: u128
}

impl Instant {
	pub fn now() -> Self {
		let nanos = unsafe { crate::bridge::time::system_time() };
		Self {
			nanos
		}
	}

	pub fn duration_since(&self, earlier: Instant) -> Duration {
		self.saturating_duration_since(earlier)
	}

	pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
		let nanos = self.nanos.checked_sub(earlier.nanos)?;
		Some(Duration::new(
			(nanos / 1_000_000_000) as u64,
			(nanos % 1_000_000_000) as u32
		))
	}

	pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
		let nanos = self.nanos.saturating_sub(earlier.nanos);
		Duration::new(
			(nanos / 1_000_000_000) as u64,
			(nanos % 1_000_000_000) as u32
		)
	}

	pub fn elapsed(&self) -> Duration {
		Self::now() - *self
	}

	pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
		let nanos = self.nanos.checked_add(duration.as_nanos())?;
		Some(Instant { nanos })
	}

	pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
		let nanos = self.nanos.checked_sub(duration.as_nanos())?;
		Some(Instant { nanos })
	}
}

impl Add<Duration> for Instant {
	type Output = Instant;

	fn add(self, rhs: Duration) -> Self::Output {
		Instant { nanos: self.nanos + rhs.as_nanos() }
	}
}

impl AddAssign<Duration> for Instant {
	fn add_assign(&mut self, rhs: Duration) {
		self.nanos += rhs.as_nanos();
	}
}

impl Sub for Instant {
	type Output = Duration;

	fn sub(self, rhs: Instant) -> Self::Output {
		self.duration_since(rhs)
	}
}

impl Sub<Duration> for Instant {
	type Output = Instant;

	fn sub(self, rhs: Duration) -> Self::Output {
		Instant { nanos: self.nanos - rhs.as_nanos() }
	}
}

impl SubAssign<Duration> for Instant {
	fn sub_assign(&mut self, rhs: Duration) {
		self.nanos -= rhs.as_nanos();
	}
}
