use core::ops::Range;
use bitflags::bitflags;
use crate::FileLocation;
use crate::newtype_enum;

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct ProgramHeaderEntry64 {
	pub segment_type: SegmentType,
	pub segment_flags: SegmentFlags,
	pub(crate) file_offset: u64,
	pub vaddr: u64,
	pub paddr: u64,
	pub file_size: u64,
	pub memory_size: u64,
	pub alignment: u64
}


impl ProgramHeaderEntry64 {
	pub fn file_location(&self) -> FileLocation {
		FileLocation(Range {
			start: usize::try_from(self.file_offset).unwrap(),
			end: usize::try_from(self.file_offset + self.file_size).unwrap()
		})
	}

	pub fn memory_location(&self) -> Range<u64> {
		Range {
			start: self.vaddr,
			end: self.vaddr + self.memory_size
		}
	}
}

newtype_enum! {
	pub enum SegmentType: u32 => {
		NULL = 0,
		LOAD = 1,
		DYNAMIC = 2,
		INTERPRETER = 3,
		NOTE = 4,
		PROGRAM_HEADER = 6,
		TLS = 7,
		OS_LOW = 0x6000_0000,
		KERNEL_MODULE_INFO = 0x6000_0000,
		OS_HIGH = 0x6FFF_FFFF,
		PROCESSOR_LOW = 0x7000_0000,
		PROCESSOR_HIGH = 0x7FFF_FFFF,
	}
}

impl SegmentType {
	pub fn new_os(value: u32) -> Option<SegmentType> {
		if (Self::OS_LOW.0..=Self::OS_HIGH.0).contains(&value) { Some(SegmentType(value)) }
		else { None }
	}

	pub fn new_processor(value: u32) -> Option<SegmentType> {
		if (Self::PROCESSOR_LOW.0..=Self::PROCESSOR_HIGH.0).contains(&value) { Some(SegmentType(value)) }
		else { None }
	}
}

bitflags! {
	#[derive(Debug, Copy, Clone)]
	#[repr(C)]
	pub struct SegmentFlags: u32 {
		const Executable = 0x1;
		const Writeable = 0x2;
        const Readable = 0x4;
		const LowMem = 0x10000;
	}
}
