use core::num::NonZero;

/// Basic operations to decide how to map memory together.
///
/// See the [module level documentation](`super`) for more information.
pub trait Mappable {
	/// The number of pages required to create a mapping with `frame_count` frames.
	fn virtual_size(&self, frame_count: usize) -> NonZero<usize>;

	/// The number of pages to offset the physical memory into the allocated virtual memory.
	fn base_virtual_offset(&self) -> isize;
}

/// A basic map of virtual to physical memory.
///
/// In certain build configurations, this may include guard pages to check for out-of-bounds access.
/// If a guaranteed amount of virtual address space usage of virtual location is required, see [`UncheckedMmap`].
///
/// <div class="warning">
///
/// When combined with [`virtual_location`](`super::Config::virtual_location`), the use of guard pages may mean that the valid start
/// of the virtual allocation is not at the requested location. Retrieve the valid start using
/// [`virtual_valid_start`](`crate::mapping::Mapping::virtual_valid_start`), [`as_ptr`](crate::mapping::Mapping::as_ptr), or similar after
/// creating the mapping to get the correct address.
///
/// </div>
#[derive(Default, Debug, Copy, Clone)]
pub struct Mmap {
	#[cfg(debug_assertions)] inner: CheckedMmap,
	#[cfg(not(debug_assertions))] inner: UncheckedMmap,
}

impl Mappable for Mmap {
	fn virtual_size(&self, frame_count: usize) -> NonZero<usize> {
		self.inner.virtual_size(frame_count)
	}

	fn base_virtual_offset(&self) -> isize {
		self.inner.base_virtual_offset()
	}
}

#[derive(Default, Debug, Copy, Clone)]
struct CheckedMmap(());

impl Mappable for CheckedMmap {
	fn virtual_size(&self, frame_count: usize) -> NonZero<usize> {
		NonZero::new(frame_count + 2).unwrap()
	}

	fn base_virtual_offset(&self) -> isize {
		1
	}
}

/// An implementation of [`Mappable`] for a stack, providing guard pages at the start and end of each allocation.
#[derive(Default, Debug, Copy, Clone)]
pub struct Stack(CheckedMmap);

impl Mappable for Stack {
	fn virtual_size(&self, frame_count: usize) -> NonZero<usize> {
		self.0.virtual_size(frame_count)
	}

	fn base_virtual_offset(&self) -> isize {
		self.0.base_virtual_offset()
	}
}

/// A basic map of virtual to physical memory, with no out-of-bounds checking.
///
/// Most uses should instead use [`Mmap`], unless a guaranteed amount of or location
/// in virtual memory is required.
#[deprecated = "renamed to `UncheckedMmap`"]
pub type UnsafeMmap = UncheckedMmap;

/// A basic map of virtual to physical memory, with no out-of-bounds checking.
///
/// Most uses should instead use [`Mmap`], unless a guaranteed amount of or location
/// in virtual memory is required.
#[derive(Default, Debug, Copy, Clone)]
pub struct UncheckedMmap(());

impl Mappable for UncheckedMmap {
	fn virtual_size(&self, frame_count: usize) -> NonZero<usize> {
		NonZero::new(frame_count).unwrap()
	}

	fn base_virtual_offset(&self) -> isize {
		0
	}
}

impl<T: ?Sized + Mappable> Mappable for Box<T> {
	fn virtual_size(&self, frame_count: usize) -> NonZero<usize> {
		T::virtual_size(&**self, frame_count)
	}

	fn base_virtual_offset(&self) -> isize {
		T::base_virtual_offset(&**self)
	}
}
