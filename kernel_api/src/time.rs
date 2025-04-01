#![unstable(feature = "kernel_time", issue = "none")]

use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

#[derive(Copy, Clone, Hash, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Instant {
	arch_val: u128
}

impl Instant {
	fn nanos(&self) -> u128 {
		Self::arch_to_nanos(self.arch_val)
	}
	
	fn arch_to_nanos(arch_val: u128) -> u128 {
		let (num, denom) = unsafe { crate::bridge::time::system_time_to_nanos() };

		arch_val * num / denom.get()
	}

	fn nanos_to_arch(nanos: u128) -> u128 {
		let (num, denom) = unsafe { crate::bridge::time::system_time_to_nanos() };

		nanos * denom.get() / num
	}
	
	pub fn now() -> Self {
		let arch_val = unsafe { crate::bridge::time::system_time() };
		Self {
			arch_val
		}
	}

	pub fn duration_since(&self, earlier: Instant) -> Duration {
		self.saturating_duration_since(earlier)
	}

	pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
		let nanos = self.nanos().checked_sub(earlier.nanos())?;
		Some(Duration::new(
			(nanos / 1_000_000_000) as u64,
			(nanos % 1_000_000_000) as u32
		))
	}

	pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
		let nanos = self.nanos().saturating_sub(earlier.nanos());
		Duration::new(
			(nanos / 1_000_000_000) as u64,
			(nanos % 1_000_000_000) as u32
		)
	}

	pub fn elapsed(&self) -> Duration {
		Self::now() - *self
	}

	pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
		let arch_val = self.arch_val.checked_add(Self::nanos_to_arch(duration.as_nanos()))?;
		Some(Instant { arch_val })
	}

	pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
		let arch_val = self.arch_val.checked_sub(Self::nanos_to_arch(duration.as_nanos()))?;
		Some(Instant { arch_val })
	}
	
	pub fn nanos_since_boot(&self) -> u128 {
		self.nanos()
	}
	
	pub fn get(&self) -> u128 { self.arch_val }
}

impl Add<Duration> for Instant {
	type Output = Instant;

	fn add(self, rhs: Duration) -> Self::Output {
		Instant { arch_val: self.arch_val + Self::nanos_to_arch(rhs.as_nanos()) }
	}
}

impl AddAssign<Duration> for Instant {
	fn add_assign(&mut self, rhs: Duration) {
		self.arch_val += Self::nanos_to_arch(rhs.as_nanos());
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
		Instant { arch_val: self.arch_val - Self::nanos_to_arch(rhs.as_nanos()) }
	}
}

impl SubAssign<Duration> for Instant {
	fn sub_assign(&mut self, rhs: Duration) {
		self.arch_val -= Self::nanos_to_arch(rhs.as_nanos());
	}
}
