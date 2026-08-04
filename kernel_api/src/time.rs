//! Temporal quantification.
//!
//! # Examples
//!
//! Using [`Instant`] to calculate how long a function took to run:
//!
//! ```ignore (incomplete)
//! # use kernel_api::time::Instant;
//! let now = Instant::now();
//!
//! // Calling a slow function, it may take a while
//! slow_function();
//!
//! let elapsed_time = now.elapsed();
//! info!("Running slow_function() took {} seconds.", elapsed_time.as_secs());
//! ```

use core::ops::{Add, AddAssign, Sub, SubAssign};
use core::time::Duration;

/// A measurement of the current system time.
/// Opaque and useful only with [`Duration`].
///
/// Instants are opaque types that should usually only be compared to one another.
/// [`get()`](Self::get) can be used to get the internal value, but no guarantees are made about
/// the meaning of the value. The meaning of the result of `get()` varies by system
/// and may even change at runtime during a single boot. A best-attempt value of the
/// time since system boot can be retrieved with [`nanos_since_boot()`](Self::nanos_since_boot).
///
/// The size of an `Instant` struct may vary depending on the target platform.
///
/// Example:
///
/// ```compile_fail (incomplete)
/// use core::time::Duration;
/// use kernel_api::time::Instant;
/// use kernel_api::threading::sleep;
/// use kernel_api::executor::block_on;
///
/// fn main() {
///    let now = Instant::now();
///
///    // we sleep for 2 seconds
///    block_on(sleep(Duration::new(2, 0)));
///    // it prints '2'
///    info!("{}", now.elapsed().as_secs());
/// }
/// ```
///
/// # Underlying system 
///
/// The following mechanisms are currently being used by `now()` to find out
/// the current time:
///
/// | Platform | Mechanism           |
/// |----------|---------------------|
/// | `x86_64` | `rdtsc` instruction |
///
/// **Disclaimer:** These mechanisms might change over time.
///
/// > Note: mathematical operations like [`add()`](`Instant::add`) may panic if the underlying
/// > structure cannot represent the new point in time.
#[derive(Copy, Clone, Hash, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Instant {
	arch_val: u128
}

impl Instant {
	fn nanos(&self) -> u128 {
		Self::arch_to_nanos(self.arch_val)
	}
	
	fn arch_to_nanos(arch_val: u128) -> u128 {
		let (num, denom) = crate::bridge::time::system_time_to_nanos();

		arch_val * num / denom.get()
	}

	fn nanos_to_arch(nanos: u128) -> u128 {
		let (num, denom) = crate::bridge::time::system_time_to_nanos();

		nanos * denom.get() / num
	}
	
	/// The current system time.
	///
	/// # Examples
	///
    /// ```
    /// use kernel_api::time::Instant;
    ///
    /// let now = Instant::now();
    /// ```
	#[must_use]
	pub fn now() -> Self {
		let arch_val = crate::bridge::time::system_time();
		Self {
			arch_val
		}
	}

	/// Returns the amount of time elapsed from another instant to this one.
	///
	/// # Panics
	///
	/// This method may panic if `self` is earlier than `earlier`.
	///
    /// # Examples
    ///
    /// ```compile_fail (incomplete)
	/// use core::time::Duration;
	/// use kernel_api::time::Instant;
	/// use kernel_api::threading::sleep;
	/// use kernel_api::executor::block_on;
    ///
    /// let now = Instant::now();
    /// block_on(sleep(Duration::new(1, 0)));
    /// let new_now = Instant::now();
    /// info!("{:?}", new_now.duration_since(now));
    /// ```
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub fn duration_since(&self, earlier: Self) -> Duration {
		self.saturating_duration_since(earlier)
	}

	/// Returns the amount of time elapsed from another instant to this one,
    /// or None if that instant is later than this one.
    ///
    /// # Examples
    ///
    /// ```compile_fail (incomplete)
	/// use core::time::Duration;
	/// use kernel_api::time::Instant;
	/// use kernel_api::threading::sleep;
	/// use kernel_api::executor::block_on;
    ///
    /// let now = Instant::now();
    /// block_on(sleep(Duration::new(1, 0)));
    /// let new_now = Instant::now();
    /// info!("{:?}", new_now.checked_duration_since(now));
    /// info!("{:?}", now.checked_duration_since(new_now)); // None
    /// ```
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
		let nanos = self.nanos().checked_sub(earlier.nanos())?;
		Some(Duration::new(
			(nanos / 1_000_000_000).truncate::<u64>(),
			(nanos % 1_000_000_000).truncate::<u32>(),
		))
	}

	/// Returns the amount of time elapsed from another instant to this one,
    /// or zero duration if that instant is later than this one.
    ///
    /// # Examples
    ///
    /// ```compile_fail (incomplete)
	/// use core::time::Duration;
	/// use kernel_api::time::Instant;
	/// use kernel_api::threading::sleep;
	/// use kernel_api::executor::block_on;
    ///
    /// let now = Instant::now();
    /// block_on(sleep(Duration::new(1, 0)));
    /// let new_now = Instant::now();
    /// info!("{:?}", new_now.saturating_duration_since(now));
    /// info!("{:?}", now.saturating_duration_since(new_now)); // 0ns
    /// ```
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
		let nanos = self.nanos().saturating_sub(earlier.nanos());
		Duration::new(
			(nanos / 1_000_000_000).truncate::<u64>(),
			(nanos % 1_000_000_000).truncate::<u32>(),
		)
	}

	/// Returns the amount of time elapsed since this instant.
    ///
    /// # Panics
    ///
    /// This method may panic if the current time is earlier than `self`.
    ///
    /// # Examples
    ///
    /// ```compile_fail (incomplete)
	/// use core::time::Duration;
	/// use kernel_api::time::Instant;
	/// use kernel_api::threading::sleep;
	/// use kernel_api::executor::block_on;
    ///
    /// let instant = Instant::now();
    /// let three_secs = Duration::from_secs(3);
    /// block_on(sleep(three_secs));
    /// assert!(instant.elapsed() >= three_secs);
    /// ```
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub fn elapsed(&self) -> Duration {
		Self::now() - *self
	}

	/// Returns `Some(t)` where `t` is the time `self + duration` if `t` can be represented as
    /// `Instant` (which means it's inside the bounds of the underlying data structure), `None`
    /// otherwise.
	#[expect(rustdoc::missing_doc_code_examples, reason = "any useful example would be unreasonable to write")]
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub fn checked_add(&self, duration: Duration) -> Option<Self> {
		let arch_val = self.arch_val.checked_add(Self::nanos_to_arch(duration.as_nanos()))?;
		Some(Self { arch_val })
	}

	/// Returns `Some(t)` where `t` is the time `self - duration` if `t` can be represented as
    /// `Instant` (which means it's inside the bounds of the underlying data structure), `None`
    /// otherwise.
	#[expect(rustdoc::missing_doc_code_examples, reason = "any useful example would be unreasonable to write")]
	#[must_use = "this returns the result of the operation, without modifying the original"]
	pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
		let arch_val = self.arch_val.checked_sub(Self::nanos_to_arch(duration.as_nanos()))?;
		Some(Self { arch_val })
	}
	
	/// The number of nanoseconds since the system booted.
	/// 
	/// No guarantees are provided on time before overflow.
	///
	/// ```
	/// use kernel_api::time::Instant;
	///
	/// let now = Instant::now();
	/// info!("system has been booted for {} ns.", now.nanos_since_boot);
	/// ```
	#[must_use]
	pub fn nanos_since_boot(&self) -> u128 {
		self.nanos()
	}
	
	/// Returns the opaque numeric value of the instant.
	/// 
	/// The meaning of the returned value is architecture dependant, see the
	/// [struct level documentation](`Instant#underlying-system`) for more information.
	#[expect(rustdoc::missing_doc_code_examples, reason = "no useful example")]
	#[must_use]
	pub const fn get(&self) -> u128 { self.arch_val }
}

impl Add<Duration> for Instant {
	type Output = Self;

	/// # Panics
    ///
    /// This function may panic if the resulting point in time cannot be represented by the
    /// underlying data structure. See [`Instant::checked_add`] for a version without panic.
	#[track_caller]
	fn add(self, rhs: Duration) -> Self::Output {
		Self { arch_val: self.arch_val + Self::nanos_to_arch(rhs.as_nanos()) }
	}
}

impl AddAssign<Duration> for Instant {
	fn add_assign(&mut self, rhs: Duration) {
		self.arch_val += Self::nanos_to_arch(rhs.as_nanos());
	}
}

impl Sub for Instant {
	type Output = Duration;

	fn sub(self, rhs: Self) -> Self::Output {
		self.duration_since(rhs)
	}
}

impl Sub<Duration> for Instant {
	type Output = Self;

	/// # Panics
    ///
    /// This function may panic if the resulting point in time cannot be represented by the
    /// underlying data structure. See [`Instant::checked_sub`] for a version without panic.
	#[track_caller]
	fn sub(self, rhs: Duration) -> Self::Output {
		Self { arch_val: self.arch_val - Self::nanos_to_arch(rhs.as_nanos()) }
	}
}

impl SubAssign<Duration> for Instant {
	fn sub_assign(&mut self, rhs: Duration) {
		self.arch_val -= Self::nanos_to_arch(rhs.as_nanos());
	}
}
