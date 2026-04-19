//! Physical and virtual memory allocation

mod pmm;
pub use pmm::*;

mod vmm;
pub use vmm::*;

/// The error returned when an allocation was unsuccessful
#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct AllocError {
	provider: AllocProvider,
}

impl AllocError {
	pub fn pmm() -> Self { Self { provider: AllocProvider::Pmm } }
	pub fn vmm() -> Self { Self { provider: AllocProvider::Vmm } }
	pub fn heap() -> Self { Self { provider: AllocProvider::Heap } }
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
		AllocError { provider: AllocProvider::Syscall(value) }
	}
}
