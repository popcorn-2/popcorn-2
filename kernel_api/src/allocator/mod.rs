//! Physical and virtual memory allocation

mod pmm;
pub use pmm::*;

mod vmm;
pub use vmm::*;

/// The error returned when an allocation was unsuccessful.
///
/// This will include a payload about the reason for failure.
///
/// # Examples
///
/// ```
/// use kernel_api::allocator::highmem;
///
/// if let Err(e) = highmem().allocate_one() {
///     info!("memory allocation failed: {e}");
/// }
/// ```
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct AllocError {
	provider: AllocProvider,
}

impl AllocError {
	/// Constructs an `AllocError` caused by an error in physical memory allocation.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::allocator::AllocError;
	///
	/// let error = AllocError::pmm();
	/// ```
	#[must_use]
	pub const fn pmm() -> Self { Self { provider: AllocProvider::Pmm } }

	/// Constructs an `AllocError` caused by an error in virtual memory allocation.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::allocator::AllocError;
	///
	/// let error = AllocError::vmm();
	/// ```
	#[must_use]
	pub const fn vmm() -> Self { Self { provider: AllocProvider::Vmm } }

	/// Constructs an `AllocError` caused by an error in heap allocation.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::allocator::AllocError;
	///
	/// let error = AllocError::heap();
	/// ```
	#[must_use]
	pub const fn heap() -> Self { Self { provider: AllocProvider::Heap } }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
enum AllocProvider {
	#[default]
	None,
	Pmm,
	Vmm,
	Heap,
	Syscall(crate::syscall::Error),
}

impl From<crate::syscall::Error> for AllocError {
	fn from(value: crate::syscall::Error) -> Self {
		Self { provider: AllocProvider::Syscall(value) }
	}
}

impl fmt::Display for AllocError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self.provider {
			AllocProvider::None => write!(f, "memory allocation failed"),
			AllocProvider::Pmm => write!(f, "physical memory allocation failed"),
			AllocProvider::Vmm => write!(f, "virtual memory allocation failed"),
			AllocProvider::Heap => write!(f, "heap memory allocation failed"),
			AllocProvider::Syscall(_) => write!(f, "memory allocation failed due to syscall error"),
		}
	}
}

impl core::error::Error for AllocError {
	fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
		match self.provider {
			AllocProvider::Syscall(ref error) => Some(error),
			_ => None,
		}
	}
}
