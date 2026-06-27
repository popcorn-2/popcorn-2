//! Physical and virtual memory allocation.

mod pmm;
pub use pmm::*;

mod vmm;
pub use vmm::*;

/// The error returned when an allocation was unsuccessful.
#[derive_const(Clone, Eq, PartialEq, Default)]
#[derive(Debug, Copy)]
#[non_exhaustive]
pub struct AllocError {
	provider: AllocProvider,
}

impl AllocError {
	/// Constructs an `AllocError` caused by an error in physical memory allocation.
	#[must_use]
	pub const fn pmm() -> Self { Self { provider: AllocProvider::Pmm } }

	/// Constructs an `AllocError` caused by an error in virtual memory allocation.
	#[must_use]
	pub const fn vmm() -> Self { Self { provider: AllocProvider::Vmm } }

	/// Constructs an `AllocError` caused by an error in heap allocation.
	#[must_use]
	pub const fn heap() -> Self { Self { provider: AllocProvider::Heap } }
}

#[derive_const(Clone, Eq, PartialEq, Default)]
#[derive(Debug, Copy)]
enum AllocProvider {
	#[default]
	None,
	Pmm,
	Vmm,
	Heap,
	Syscall(crate::syscall::Error),
}

impl const From<crate::syscall::Error> for AllocError {
	fn from(value: crate::syscall::Error) -> Self {
		Self { provider: AllocProvider::Syscall(value) }
	}
}
