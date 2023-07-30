use alloc::vec::Vec;
use core::fmt::{Formatter, Pointer};
use kernel_exports::memory::{Frame, PhysicalAddress, PhysicalMemoryAllocator};

#[derive(Debug)]
#[repr(C)]
pub struct Data {
	pub framebuffer: Option<Framebuffer>,
	pub memory: Memory,
	pub modules: Modules,
	pub log: Logging,
	pub test: Testing
}

#[derive(Debug)]
#[repr(C)]
pub struct Framebuffer {
	pub buffer: *mut u8,
	pub stride: usize,
	pub width: usize,
	pub height: usize,
	pub color_format: ColorMask
}

#[derive(Debug)]
#[repr(C)]
pub struct ColorMask {
	pub red: u32, pub green: u32, pub blue: u32
}

impl ColorMask {
	pub const RGBX: Self = Self { red: 0xFF << 24, green: 0xFF << 16, blue: 0xFF << 8 };
	pub const BGRX: Self = Self { red: 0xFF << 8, green: 0xFF << 16, blue: 0xFF << 24 };
}

#[derive(Debug)]
#[repr(C)]
pub struct Memory {
	pub map: Vec<MemoryMapEntry>,
	pub page_table_root: Frame
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemoryMapEntry {
	pub coverage: Range<PhysicalAddress>,
	pub ty: MemoryType
}

impl MemoryMapEntry {
	pub fn start(self) -> PhysicalAddress { self.coverage.0 }
	pub fn end(self) -> PhysicalAddress { self.coverage.1 }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Range<T>(pub T, pub T);

impl<T> Range<T> {
	pub fn start(self) -> T { self.0 }
	pub fn end(self) -> T { self.1 }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(i64)]
pub enum MemoryType {
	Reserved = -1,
	Free,
	BootloaderCode,
	BootloaderData,
	KernelCode,
	KernelData,
	KernelStack,
	KernelPageTable,
	ModuleCode,
	ModuleData,
	AcpiPreserve,
	AcpiReclaim,
}

#[repr(C)]
pub struct Modules {
	pub phys_allocator_start: extern "sysv64" fn(Range<Frame>) -> Result<&'static dyn PhysicalMemoryAllocator, ()>
}

impl core::fmt::Debug for Modules {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		<(*const ()) as core::fmt::Pointer>::fmt(&{self.phys_allocator_start as *const ()}, f)
	}
}

#[derive(Debug)]
#[repr(C)]
pub struct Logging;

#[repr(C)]
pub struct Testing {
	pub module_func: extern "sysv64" fn()
}

impl core::fmt::Debug for Testing {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		<(*const ()) as core::fmt::Pointer>::fmt(&{self.module_func as *const ()}, f)
	}
}
