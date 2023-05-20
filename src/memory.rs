use core::fmt;

pub struct PhysAddr(usize);
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
