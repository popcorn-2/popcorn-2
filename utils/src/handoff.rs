use core::fmt::{Debug, Formatter};
use core::ptr::NonNull;
use kernel_api::memory::{PhysicalAddress, RawFrame, RawPage, VirtualAddress};
use core::range::Range;

#[repr(C)]
pub struct Data {
	pub framebuffer: Framebuffer,
	pub memory: Memory,
	pub log: Logging,
	pub rsdp: PhysicalAddress,
	pub init_entry: VirtualAddress,
}

impl Debug for Data {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Data")
				.field("framebuffer", &self.framebuffer)
				.field("memory", &self.memory)
				.field("log", &self.log)
				.field("rsdp", &self.rsdp)
		        .finish_non_exhaustive()
	}
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Framebuffer {
	pub buffer: *mut u8,
	pub stride: usize,
	pub width: usize,
	pub height: usize,
	pub color_format: ColorMask,
	pub physical_address: PhysicalAddress,
}

impl Debug for Framebuffer {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Framebuffer")
				.field_with("buffer", |f| {
					write!(f, "{:p}", self.buffer)
				})
				.field("stride", &self.stride)
				.field("width", &self.width)
				.field("height", &self.height)
				.field("color_format", &self.color_format)
				.field("physical_address", &self.physical_address)
				.finish()
	}
}

#[derive(Debug, Copy, Clone)]
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
	pub map: Range<PhysicalAddress>,
	pub lowest_used: RawPage,
	pub stack: Stack,
	pub s_table: RawFrame,
	pub u_tables: [RawFrame; 2],
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Stack {
	pub page_count: usize,
	pub bottom_virt: RawPage,
	pub bottom_phys: RawFrame,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemoryMapEntry {
	pub coverage: Range<PhysicalAddress>,
	pub ty: MemoryType
}

impl MemoryMapEntry {
	pub fn start(self) -> PhysicalAddress { self.coverage.start }
	pub fn end(self) -> PhysicalAddress { self.coverage.end }
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
	RuntimeCode,
	RuntimeData
}

#[repr(C)]
pub struct Logging {
	pub symbol_map: Option<NonNull<[u8]>>
}

impl Debug for Logging {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Logging")
				.finish_non_exhaustive()
	}
}
