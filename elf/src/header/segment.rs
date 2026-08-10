//! Types for reading ELF program headers.

use crate::raw::index_part;
use crate::{raw, file::Header as FileHeader, Width};
use bitflags::bitflags;
use kernel_api::memory::{PhysicalAddress, VirtualAddress};

/// The error returned when parsing a [`Segment`].
#[derive(Debug, Copy, Clone)]
pub enum ParseError {
	/// The passed file is too short for the expected length of a program header entry.
	NotEnoughData,
	/// The passed file contains invalid data.
	InvalidFile,
}

/// A parsed ELF program header entry.
#[derive(Debug, Copy, Clone)]
pub struct Segment {
	ty: Type,
	flags: Flags,
	file_offset: u64,
	vaddr: u64,
	paddr: u64,
	file_size: u64,
	mem_size: u64,
	align: u64,
}

impl Segment {
	/// Parses a segment descriptor from the raw contents of a program header entry.
	///
	/// The parsed file [`Header`](FileHeader) must be passed to determine the correct [ELF Class](Width)
	/// and [`Endianness`](crate::Endianness) to use.
	///
	/// # Errors
	///
	/// Returns a [`ParseError`] if the entry could not be parsed, or the file [`Header`](FileHeader) contained
	/// unsupported values.
	pub fn try_new<D: AsRef<[u8]>>(from: D, with: &FileHeader) -> Result<Self, ParseError> {
		let entry = from.as_ref();

		match with.width() {
			Width::X32 if entry.len() < raw::program::x32::SIZE => return Err(ParseError::NotEnoughData),
			Width::X64 if entry.len() < raw::program::x64::SIZE => return Err(ParseError::NotEnoughData),
			Width::X32 | Width::X64 => {},
			_ => return Err(ParseError::InvalidFile),
		}

		let segment = match with.width() {
			Width::X32 => {
				let ty = Type(with.endianness().decode_u32(index_part::<raw::program::x32::Type>(entry)));
				let offset = with.endianness().decode_u32(index_part::<raw::program::x32::Offset>(entry));
				let vaddr = with.endianness().decode_u32(index_part::<raw::program::x32::VAddr>(entry));
				let paddr = with.endianness().decode_u32(index_part::<raw::program::x32::PAddr>(entry));
				let file_size = with.endianness().decode_u32(index_part::<raw::program::x32::FileSize>(entry));
				let mem_size = with.endianness().decode_u32(index_part::<raw::program::x32::MemSize>(entry));
				let flags = Flags::from_bits_retain(with.endianness().decode_u32(index_part::<raw::program::x32::Flags>(entry)));
				let align = with.endianness().decode_u32(index_part::<raw::program::x32::Align>(entry));

				Self {
					ty,
					flags,
					file_offset: u64::from(offset),
					vaddr: u64::from(vaddr),
					paddr: u64::from(paddr),
					file_size: u64::from(file_size),
					mem_size: u64::from(mem_size),
					align: u64::from(align),
				}
			},
			Width::X64 => {
				let ty = Type(with.endianness().decode_u32(index_part::<raw::program::x64::Type>(entry)));
				let offset = with.endianness().decode_u64(index_part::<raw::program::x64::Offset>(entry));
				let vaddr = with.endianness().decode_u64(index_part::<raw::program::x64::VAddr>(entry));
				let paddr = with.endianness().decode_u64(index_part::<raw::program::x64::PAddr>(entry));
				let file_size = with.endianness().decode_u64(index_part::<raw::program::x64::FileSize>(entry));
				let mem_size = with.endianness().decode_u64(index_part::<raw::program::x64::MemSize>(entry));
				let flags = Flags::from_bits_retain(with.endianness().decode_u32(index_part::<raw::program::x64::Flags>(entry)));
				let align = with.endianness().decode_u64(index_part::<raw::program::x64::Align>(entry));

				Self {
					ty,
					flags,
					file_offset: offset,
					vaddr,
					paddr,
					file_size,
					mem_size,
					align,
				}
			},
			_ => return Err(ParseError::InvalidFile),
		};

		Ok(segment)
	}

	/// The type of this segment.
	#[must_use]
	pub const fn ty(&self) -> Type {
		self.ty
	}

	/// Additional information about how to load this segment.
	#[must_use]
	pub const fn flags(&self) -> Flags {
		self.flags
	}

	#[doc(hidden)]
	#[must_use]
	pub const fn file_offset(&self) -> u64 {
		self.file_offset
	}

	/// The virtual address this segment should be loaded at.
	#[must_use]
	pub const fn vaddr(&self) -> VirtualAddress {
		// fixme
		VirtualAddress::new(self.vaddr as usize)
	}

	/// The physical address this segment should be loaded at.
	#[must_use]
	pub const fn paddr(&self) -> PhysicalAddress {
		// fixme
		PhysicalAddress::new(self.paddr as usize)
	}

	#[doc(hidden)]
	#[must_use]
	pub const fn file_size(&self) -> u64 {
		self.file_size
	}

	/// Size this segment takes up once loaded into memory.
	///
	/// > **Note**: This may be larger than the length of the data contained
	/// > in this segment. The remaining space should be filled with zeroes.
	#[must_use]
	pub const fn mem_size(&self) -> usize {
		// fixme
		self.mem_size as usize
	}

	/// Required alignment of the start of this segment.
	#[must_use]
	pub const fn align(&self) -> usize {
		// fixme
		self.align as usize
	}
}

/// An iterator of each [`Segment`] in a file.
#[derive(Debug)]
pub struct Iter<'f> {
	entries: &'f [u8],
	header: &'f FileHeader,
}

impl<'f> Iter<'f> {
	pub(crate) const fn new(entries: &'f [u8], header: &'f FileHeader) -> Self {
		Self {
			entries,
			header,
		}
	}
}

impl Iterator for Iter<'_> {
	type Item = Segment;

	fn next(&mut self) -> Option<Self::Item> {
		let (entry, rest) = self.entries.split_at_checked(self.header.program_header_entry_size().into())?;
		self.entries = rest;

		Segment::try_new(entry, self.header).ok()
	}

	fn nth(&mut self, n: usize) -> Option<Self::Item> {
		let entry_size = usize::from(self.header.program_header_entry_size());
		let (_, rest) = self.entries.split_at_checked(entry_size * n)?;
		self.entries = rest;
		self.next()
	}
}

kernel_api::newtype_enum! {
	/// Type of segment in program header table.
	pub enum Type: u32 => {
		/// Unused entry.
		NULL = 0,
		/// Segment to be loaded.
		LOAD = 1,
		/// Dynamic linking table.
		DYNAMIC = 2,
		/// Interpreter path.
		INTERPRETER = 3,
		/// Auxiliary data.
		NOTE = 4,
		/// Program header table.
		PROGRAM_HEADER = 6,
		/// TLS template data.
		TLS = 7,
		/// Lowest value reserved for OS use.
		OS_LOW = 0x6000_0000,
		/// Popcorn kernel module info segment.
		KERNEL_MODULE_INFO = 0x6000_1000,
		/// Read-only segment which requires relocation.
		GNU_RELRO = 0x6474_E552,
		/// EH-frame segment.
		GNU_EH_FRAME = 0x6474_E550,
		/// Non-executable stack marking.
		GNU_STACK = 0x6474_E551,
		/// Highest value reserved for OS use.
		OS_HIGH = 0x6FFF_FFFF,
		/// Lowest value reserved for architecture specific use.
		PROCESSOR_LOW = 0x7000_0000,
		/// Highest value reserved for architecture specific use.
		PROCESSOR_HIGH = 0x7FFF_FFFF,
	}
}

impl Type {
	/// Create a new `Type` with the given value.
	///
	/// If the value falls outside the OS specific reserved values, returns `None`.
	#[must_use]
	pub fn new_os(value: u32) -> Option<Self> {
		if (Self::OS_LOW.0..=Self::OS_HIGH.0).contains(&value) { Some(Self(value)) }
		else { None }
	}

	/// Create a new `Type` with the given value.
	///
	/// If the value falls outside the architecture specific reserved values, returns `None`.
	#[must_use]
	pub fn new_processor(value: u32) -> Option<Self> {
		if (Self::PROCESSOR_LOW.0..=Self::PROCESSOR_HIGH.0).contains(&value) { Some(Self(value)) }
		else { None }
	}
}

bitflags! {
	/// Flags containing additional information describing how to load a segment.
	#[derive(Debug, Copy, Clone)]
	#[repr(C)]
	pub struct Flags: u32 {
		/// Executable code.
		const Executable = 0x1;
		/// Writeable data.
		const Writeable = 0x2;
		/// Readable data.
        const Readable = 0x4;
		/// Must be loaded below 1MiB physical address.
		#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
		const LowMem = 0x10000;
        /// OS specific flags.
		const OsMask = 0x00FF_0000;
        /// Architecture specific flags.
		const ProcessorMask = 0xFF00_0000;
	}
}

impl Flags {
	/// Create a new `SegmentFlags` with the given value.
	///
	/// If the value falls outside the OS specific reserved values, returns `None`.
	#[must_use]
	pub const fn new_os(value: u32) -> Option<Self> {
		let masked = value & Self::OsMask.bits();
		if masked == value { Some(Self::from_bits_retain(value)) }
		else { None }
	}

	/// Create a new `SegmentFlags` with the given value.
	///
	/// If the value falls outside the architecture specific reserved values, returns `None`.
	#[must_use]
	pub const fn new_processor(value: u32) -> Option<Self> {
		let masked = value & Self::ProcessorMask.bits();
		if masked == value { Some(Self::from_bits_retain(value)) }
		else { None }
	}
}
