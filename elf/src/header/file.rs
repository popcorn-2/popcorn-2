use crate::raw;
use crate::raw::index_part;
use crate::{Abi, Endianness, Isa, OsAbi, Type, Width};
use core::fmt;
use core::ops::Range;

#[derive(Debug, Copy, Clone)]
pub struct Header {
	endianness: Endianness,
	abi: Abi,
	file_type: Type,
	isa: Isa,
	extra: ExtraHeader,
}

impl Header {
	const NON_EXTRA_LEN: usize = 0x18;
	const X32_EXTRA_LEN: usize = 0x34 - Self::NON_EXTRA_LEN;
	const X64_EXTRA_LEN: usize = 0x40 - Self::NON_EXTRA_LEN;

	pub fn try_new(data: &[u8]) -> Result<Self, Error> {
		if data.len() <= Self::NON_EXTRA_LEN {
			return Err(Error::NotEnoughData);
		}

		let magic = index_part::<raw::file::Magic>(data);
		if magic != [0x7f, b'E', b'L', b'F'] {
			return Err(Error::InvalidMagic(magic));
		}

		let endianness = Endianness(index_part::<raw::file::Data>(data)[0]);
		match endianness {
			Endianness::BIG | Endianness::LITTLE => {},
			Endianness(endianness) => return Err(Error::InvalidEndianness(endianness)),
		}

		let width = Width(endianness.decode_u8(index_part::<raw::file::Class>(data)));
		let header_version = endianness.decode_u8(index_part::<raw::file::HeaderVersion>(data));
		let abi = Abi {
			os: OsAbi(endianness.decode_u8(index_part::<raw::file::OsAbi>(data))),
			version: endianness.decode_u8(index_part::<raw::file::AbiVersion>(data)),
		};
		let ty = Type(endianness.decode_u16(index_part::<raw::file::Type>(data)));
		let isa = Isa(endianness.decode_u16(index_part::<raw::file::Machine>(data)));
		let elf_version = endianness.decode_u32(index_part::<raw::file::FileVersion>(data));

		let extra = match width {
			Width::X32 => {
				if data.len() < Self::NON_EXTRA_LEN + Self::X32_EXTRA_LEN {
					return Err(Error::NotEnoughData);
				}

				ExtraHeader::X32(FileHeaderExtra {
					entry_point: endianness.decode_u32(index_part::<raw::file::x32::Entry>(data)),
					program_header_offset: endianness.decode_u32(index_part::<raw::file::x32::ProgramHeaderOffset>(data)),
					section_header_offset: endianness.decode_u32(index_part::<raw::file::x32::SectionHeaderOffset>(data)),
					flags: endianness.decode_u32(index_part::<raw::file::x32::Flags>(data)),
					header_size: endianness.decode_u16(index_part::<raw::file::x32::HeaderSize>(data)),
					program_header_entry_size: endianness.decode_u16(index_part::<raw::file::x32::ProgramHeaderEntrySize>(data)),
					program_header_entry_count: endianness.decode_u16(index_part::<raw::file::x32::ProgramHeaderEntryNum>(data)),
					section_header_entry_size: endianness.decode_u16(index_part::<raw::file::x32::SectionHeaderEntrySize>(data)),
					section_header_entry_count: endianness.decode_u16(index_part::<raw::file::x32::SectionHeaderEntryNum>(data)),
					section_string_table: endianness.decode_u16(index_part::<raw::file::x32::SectionHeaderStrTabIndex>(data)),
				})
			},
			Width::X64 => {
				if data.len() < Self::NON_EXTRA_LEN + Self::X64_EXTRA_LEN {
					return Err(Error::NotEnoughData);
				}

				ExtraHeader::X64(FileHeaderExtra {
					entry_point: endianness.decode_u64(index_part::<raw::file::x64::Entry>(data)),
					program_header_offset: endianness.decode_u64(index_part::<raw::file::x64::ProgramHeaderOffset>(data)),
					section_header_offset: endianness.decode_u64(index_part::<raw::file::x64::SectionHeaderOffset>(data)),
					flags: endianness.decode_u32(index_part::<raw::file::x64::Flags>(data)),
					header_size: endianness.decode_u16(index_part::<raw::file::x64::HeaderSize>(data)),
					program_header_entry_size: endianness.decode_u16(index_part::<raw::file::x64::ProgramHeaderEntrySize>(data)),
					program_header_entry_count: endianness.decode_u16(index_part::<raw::file::x64::ProgramHeaderEntryNum>(data)),
					section_header_entry_size: endianness.decode_u16(index_part::<raw::file::x64::SectionHeaderEntrySize>(data)),
					section_header_entry_count: endianness.decode_u16(index_part::<raw::file::x64::SectionHeaderEntryNum>(data)),
					section_string_table: endianness.decode_u16(index_part::<raw::file::x64::SectionHeaderStrTabIndex>(data)),
				})
			},
			Width(width) => return Err(Error::UnknownWidth(width)),
		};

		if header_version != 1 { return Err(Error::UnknownHeaderVersion(header_version)); }
		if elf_version != 1 { return Err(Error::UnknownElfVersion(elf_version)); }

		Ok(Self {
			endianness,
			abi,
			file_type: ty,
			isa,
			extra,
		})
	}

	#[must_use]
	// fixme: usize on 32 bit arch with 64 bit elf
	pub fn program_header(&self) -> Range<usize> {
		let start = self.program_header_offset();
		let size = usize::from(self.program_header_entry_size() * self.program_header_entry_count());

		Range {start, end: start + size }
	}

	#[must_use]
	// fixme: usize on 32 bit arch with 64 bit elf
	pub fn section_header(&self) -> Range<usize> {
		let start = self.section_header_offset();
		let size = usize::from(self.section_header_entry_size() * self.section_header_entry_count());

		Range {start, end: start + size}
	}

	#[must_use]
	// fixme: usize on 32 bit arch with 64 bit elf
	pub const fn entry_point(&self) -> usize {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.entry_point as usize,
			ExtraHeader::X64(extra) => extra.entry_point as usize,
		}
	}

	#[must_use]
	// fixme: usize on 32 bit arch with 64 bit elf
	pub const fn program_header_offset(&self) -> usize {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.program_header_offset as usize,
			ExtraHeader::X64(extra) => extra.program_header_offset as usize,
		}
	}

	#[must_use]
	// fixme: usize on 32 bit arch with 64 bit elf
	pub const fn section_header_offset(&self) -> usize {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.section_header_offset as usize,
			ExtraHeader::X64(extra) => extra.section_header_offset as usize,
		}
	}

	#[must_use]
	pub const fn flags(&self) -> u32 {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.flags,
			ExtraHeader::X64(extra) => extra.flags,
		}
	}

	#[must_use]
	pub const fn header_size(&self) -> u16 {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.header_size,
			ExtraHeader::X64(extra) => extra.header_size,
		}
	}

	#[must_use]
	pub const fn program_header_entry_size(&self) -> u16 {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.program_header_entry_size,
			ExtraHeader::X64(extra) => extra.program_header_entry_size,
		}
	}

	#[must_use]
	pub const fn program_header_entry_count(&self) -> u16 {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.program_header_entry_count,
			ExtraHeader::X64(extra) => extra.program_header_entry_count,
		}
	}

	#[must_use]
	pub const fn section_header_entry_size(&self) -> u16 {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.section_header_entry_size,
			ExtraHeader::X64(extra) => extra.section_header_entry_size,
		}
	}

	#[must_use]
	pub const fn section_header_entry_count(&self) -> u16 {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.section_header_entry_count,
			ExtraHeader::X64(extra) => extra.section_header_entry_count,
		}
	}

	#[must_use]
	pub const fn section_string_table(&self) -> u16 {
		match &self.extra {
			ExtraHeader::X32(extra) => extra.section_string_table,
			ExtraHeader::X64(extra) => extra.section_string_table,
		}
	}

	#[must_use]
	pub const fn width(&self) -> Width {
		match &self.extra {
			ExtraHeader::X32(_) => Width::X32,
			ExtraHeader::X64(_) => Width::X64,
		}
	}

	#[must_use]
	pub const fn endianness(&self) -> Endianness { self.endianness }

	#[must_use]
	pub const fn abi(&self) -> Abi { self.abi }

	#[must_use]
	pub const fn isa(&self) -> Isa { self.isa }

	#[must_use]
	pub const fn ty(&self) -> Type { self.file_type }
}

#[derive(Debug, Copy, Clone)]
enum ExtraHeader {
	X32(FileHeaderExtra<u32>),
	X64(FileHeaderExtra<u64>),
}

#[derive(Debug, Copy, Clone)]
struct FileHeaderExtra<T> {
	entry_point: T,
	program_header_offset: T,
	section_header_offset: T,
	flags: u32,
	header_size: u16,
	program_header_entry_size: u16,
	program_header_entry_count: u16,
	section_header_entry_size: u16,
	section_header_entry_count: u16,
	section_string_table: u16
}

#[derive(Debug, Copy, Clone)]
pub enum Error {
	NotEnoughData,
	InvalidMagic([u8; 4]),
	InvalidEndianness(u8),
	UnknownWidth(u8),
	UnknownHeaderVersion(u8),
	UnknownElfVersion(u32),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::NotEnoughData => write!(f, "not enough data"),
			Self::InvalidEndianness(_) => write!(f, "invalid ELF endianness"),
			Self::InvalidMagic(_) => write!(f, "invalid ELF file magic"),
			Self::UnknownWidth(_) => write!(f, "unknown architecture width"),
			Self::UnknownHeaderVersion(_) => write!(f, "unknown ELF version"),
			Self::UnknownElfVersion(_) => write!(f, "unknown ELF version"),
		}
	}
}

impl core::error::Error for Error {}

impl Endianness {
	#[expect(clippy::unused_self, reason = "consistency")]
	pub(crate) const fn decode_u8(self, bytes: [u8; 1]) -> u8 { bytes[0] }

	pub(crate) const fn decode_u16(self, bytes: [u8; 2]) -> u16 {
		match self {
			Self::LITTLE => u16::from_le_bytes(bytes),
			Self::BIG => u16::from_be_bytes(bytes),
			_ => panic!("not implemented: unknown endianness"),
		}
	}

	pub(crate) const fn decode_u32(self, bytes: [u8; 4]) -> u32 {
		match self {
			Self::LITTLE => u32::from_le_bytes(bytes),
			Self::BIG => u32::from_be_bytes(bytes),
			_ => panic!("not implemented: unknown endianness"),
		}
	}

	pub(crate) const fn decode_u64(self, bytes: [u8; 8]) -> u64 {
		match self {
			Self::LITTLE => u64::from_le_bytes(bytes),
			Self::BIG => u64::from_be_bytes(bytes),
			_ => panic!("not implemented: unknown endianness"),
		}
	}
}
