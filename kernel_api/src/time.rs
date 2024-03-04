#![unstable(feature = "kernel_time", issue = "none")]

use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

#[derive(Copy, Clone, Hash, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Instant {
	time_since_boot_ns: u128
}

impl Instant {
	pub fn now() -> Self {
		let nanos = unsafe { crate::bridge::time::__popcorn_nanoseconds_since_boot() };
		Self {
			time_since_boot_ns: nanos
		}
	}

	pub fn duration_since(&self, earlier: Instant) -> Duration {
		self.saturating_duration_since(earlier)
	}

	pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
		let nanos = self.time_since_boot_ns.checked_sub(earlier.time_since_boot_ns)?;
		Some(Duration::new(
			(nanos / 1_000_000_000) as u64,
			(nanos % 1_000_000_000) as u32
		))
	}

	pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
		let nanos = self.time_since_boot_ns.saturating_sub(earlier.time_since_boot_ns);
		Duration::new(
			(nanos / 1_000_000_000) as u64,
			(nanos % 1_000_000_000) as u32
		)
	}

	pub fn elapsed(&self) -> Duration {
		Self::now() - *self
	}

	pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
		let nanos = self.time_since_boot_ns.checked_add(duration.as_nanos())?;
		Some(Instant { time_since_boot_ns: nanos })
	}

	pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
		let nanos = self.time_since_boot_ns.checked_sub(duration.as_nanos())?;
		Some(Instant { time_since_boot_ns: nanos })
	}
}

impl Add<Duration> for Instant {
	type Output = Instant;

	fn add(self, rhs: Duration) -> Self::Output {
		Instant { time_since_boot_ns: self.time_since_boot_ns + rhs.as_nanos() }
	}
}

impl AddAssign<Duration> for Instant {
	fn add_assign(&mut self, rhs: Duration) {
		self.time_since_boot_ns += rhs.as_nanos();
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
		Instant { time_since_boot_ns: self.time_since_boot_ns - rhs.as_nanos() }
	}
}

impl SubAssign<Duration> for Instant {
	fn sub_assign(&mut self, rhs: Duration) {
		self.time_since_boot_ns -= rhs.as_nanos();
	}
}
