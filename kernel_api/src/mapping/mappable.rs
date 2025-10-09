use alloc::boxed::Box;
use core::num::NonZero;

/// Basic operations to decide how to map memory together.
///
/// Implementations of this can be used to instantiate a [`RawMapping`].
pub trait Mappable {
	/// The number of pages required to create a mapping with `frame_count` frames
	fn virtual_size(&self, frame_count: usize) -> NonZero<usize>;

	/// The number of pages to offset the physical memory into the allocated virtual memory
	fn base_virtual_offset(&self) -> isize;
}

#[non_exhaustive]
#[derive(Default)]
pub struct Mmap {}

impl Mappable for Mmap {
	fn virtual_size(&self, frame_count: usize) -> NonZero<usize> {
		NonZero::new(frame_count + 2).unwrap()
	}

	fn base_virtual_offset(&self) -> isize {
		1
	}
}

pub type Stack = Mmap;

#[non_exhaustive]
#[derive(Default)]
pub struct UnsafeMmap {}

impl Mappable for UnsafeMmap {
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
