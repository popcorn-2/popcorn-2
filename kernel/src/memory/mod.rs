pub use kernel_exports::memory::{Frame, Page, PhysicalAddress, VirtualAddress, PhysicalMemoryAllocator};

/*use core::fmt;

#[repr(transparent)]
pub struct PhysAddr(usize);
#[repr(transparent)]
pub struct VirtAddr(usize);

impl From<u32> for PhysAddr {
	fn from(value: u32) -> Self {
		PhysAddr(value.try_into().unwrap())
	}
}

impl From<usize> for PhysAddr {
	fn from(value: usize) -> Self {
		PhysAddr(value)
	}
}

impl Into<usize> for PhysAddr {
	fn into(self) -> usize {
		self.0
	}
}

impl Into<VirtAddr> for PhysAddr {
	fn into(self) -> VirtAddr {
		VirtAddr(self.0 + super::PAGE_OFFSET_START)
	}
}

impl fmt::Pointer for PhysAddr {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_fmt(format_args!("PhysAddr({:#x})", self.0))
	}
}

impl From<u32> for VirtAddr {
	fn from(value: u32) -> Self {
		VirtAddr(value.try_into().unwrap())
	}
}

impl From<usize> for VirtAddr {
	fn from(value: usize) -> Self {
		VirtAddr(value)
	}
}

impl<T> From<*const T> for VirtAddr {
	fn from(value: *const T) -> Self {
		VirtAddr::from(value as usize)
	}
}

impl<T> From<*mut T> for VirtAddr {
	fn from(value: *mut T) -> Self {
		VirtAddr::from(value as usize)
	}
}

impl fmt::Pointer for VirtAddr {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_fmt(format_args!("VirtAddr({:#x})", self.0))
	}
}
*/

pub mod watermark_allocator;
pub mod paging;

pub mod alloc {
	pub mod phys {
		use core::alloc::AllocError;
		use core::mem;
		use core::ptr::{self, NonNull};
		use kernel_exports::memory::PhysicalMemoryAllocator;
		use kernel_exports::sync::RwLock;
		use super::super::Frame;

		pub static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator::new();

		struct GlobalAllocator {
			alloc: RwLock<Option<&'static mut dyn PhysicalMemoryAllocator>>
		}

		impl GlobalAllocator {
			pub const fn new() -> Self {
				Self { alloc: RwLock::new(None) }
			}

			pub unsafe fn set_unchecked<'a>(&self, allocator: &'a mut dyn PhysicalMemoryAllocator) {
				let mut alloc = self.alloc.lock();
				let thing =  mem::transmute::<_, &'static mut dyn PhysicalMemoryAllocator>(allocator);
				*alloc = Some(thing);
			}

			pub fn set(&self, allocator: &'static mut dyn PhysicalMemoryAllocator) {
				unsafe { self.set_unchecked(allocator) }
			}

			pub fn unset(&self) {
				*self.alloc.lock() = None;
			}

			pub fn try_allocate(&self, page_count: usize) -> Result<Frame, GlobalAllocError> {
				match *self.alloc.lock() {
					Some(alloc) => Ok(alloc.allocate_contiguous(page_count)?),
					None => Err(GlobalAllocError::NoAllocator)
				}
			}
		}

		pub enum GlobalAllocError {
			NoAllocator,
			AllocatorError(AllocError)
		}

		impl From<AllocError> for GlobalAllocError {
			fn from(value: AllocError) -> Self {
				Self::AllocatorError(value)
			}
		}

		pub struct Global;

		pub struct FrameBox<A = Global> {
			start: Frame,
			end: Frame,
			allocator: A
		}

		impl FrameBox<Global> {
			pub fn new() -> Self {
				todo!()
			}
		}

		impl<A> FrameBox<A> {
			pub fn new_in(allocator: A) -> Self {
				todo!()
			}
		}

		impl<A> Drop for FrameBox<A> {
			fn drop(&mut self) {
				todo!()
			}
		}
	}

	pub mod virt {

	}
}
