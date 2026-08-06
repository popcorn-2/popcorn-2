//! An ELF parser, supporting zero-copy parsing, relocation, and load-time dynamic linking of 32
//! and 64 bit ELF files.
//!
//! ELF files can be loaded from any buffer implementing [`AsRef<[u8]>`], and the various headers
//! parsed.
//!
//! # Examples
//!
//! Load an ELF file and printing header information:
//! ```no_run
//! let file = std::fs::read("elf_file").expect("Failed to read file");
//! let elf = elf::File::try_new(file).expect("Failed to parse ELF file");
//!
//! println!("==== File ====");
//! println!("Width: {:?}", elf.width());
//! println!("Data: {:?}", elf.endianness());
//! println!("Abi: {:?}", elf.abi());
//! println!("Isa: {:?}", elf.isa());
//! println!("Type: {:?}", elf.ty());
//! println!();
//!
//! println!("==== Segments ====");
//! for (i, segment) in elf.segments().enumerate() {
//!     println!("{i}: {:#x?}", segment);
//! }
//! println!();
//!
//! println!("==== Sections ====");
//! for (i, section) in elf.sections().enumerate() {
//!     println!("{i}: {:#x?}", section);
//! }
//! println!();
//! ```

#![feature(strict_provenance_lints)]
#![no_std]

use core::fmt;
use header::file;
use kernel_api::newtype_enum;

mod header;
mod raw;

pub use header::file::Header as FileHeader;
pub use header::segment;
pub use header::section;

#[derive(Debug, Clone, Copy)]
pub enum ParseError {
	FileHeader(file::Error),
}

impl fmt::Display for ParseError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::FileHeader(error) => write!(f, "File Header Error: {error}"),
		}
	}
}

impl From<file::Error> for ParseError {
	fn from(err: file::Error) -> Self {
		Self::FileHeader(err)
	}
}

impl core::error::Error for ParseError {
	fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
		match self {
			Self::FileHeader(err) => Some(err),
		}
	}
}

#[derive(Debug, Clone, Copy)]
pub struct File<D> {
	inner: FileInner<D>,
}

#[derive(Debug, Clone, Copy)]
struct FileInner<D> {
	file_header: FileHeader,
	data: D,
}

impl<D: AsRef<[u8]>> File<D> {
	pub fn try_new(from: D) -> Result<Self, ParseError> {
		let data = from.as_ref();
		let file_header = FileHeader::try_new(data)?;

		Ok(Self {
			inner: FileInner {
				file_header,
				data: from,
			},
		})
	}
}

impl<D> File<D> {
	pub const fn width(&self) -> Width { self.inner.file_header.width() }

	pub const fn endianness(&self) -> Endianness { self.inner.file_header.endianness() }

	pub const fn abi(&self) -> Abi { self.inner.file_header.abi() }

	pub const fn isa(&self) -> Isa { self.inner.file_header.isa() }

	pub const fn ty(&self) -> Type { self.inner.file_header.ty() }
}

impl<D: AsRef<[u8]>> File<D> {
	pub fn segments(&self) -> segment::Iter<'_> {
		let data = self.inner.data.as_ref();
		let entries = &data[self.inner.file_header.program_header()];
		segment::Iter::new(
			entries,
			0,
			self.width(),
			self.endianness(),
			self.inner.file_header.program_header_entry_size(),
		)
	}

	pub fn sections(&self) -> section::Iter<'_, D> {
		section::Iter::new(
			&self.inner,
			0,
			self.inner.file_header.section_header(),
		)
	}
}

impl<D: AsRef<[u8]>> FileInner<D> {
	fn section_header(&self, idx: u32) -> &[u8] {
		let section_idx = usize::try_from(idx).expect("not supported on 16 bit");
		let section_start = section_idx * usize::from(self.file_header.section_header_entry_size());
		let section_end = (section_idx + 1) * usize::from(self.file_header.section_header_entry_size());
		&self.data.as_ref()[self.file_header.section_header()][section_start..section_end]
	}
}

impl<D: AsRef<[u8]>> core::ops::Index<segment::Segment> for FileInner<D> {
	type Output = [u8];

	fn index(&self, segment: segment::Segment) -> &Self::Output {
		let start = usize::try_from(segment.file_offset()).expect("ELF file too large");
		let end = usize::try_from(segment.file_offset() + segment.file_size()).expect("ELF file too large");
		&self.data.as_ref()[start..end]
	}
}

impl<D: AsRef<[u8]>> core::ops::Index<segment::Segment> for File<D> {
	type Output = [u8];

	fn index(&self, segment: segment::Segment) -> &Self::Output {
		&self.inner[segment]
	}
}

impl<D: AsRef<[u8]>> core::ops::Index<section::Section<'_, D>> for FileInner<D> {
	type Output = [u8];

	fn index(&self, section: section::Section<'_, D>) -> &Self::Output {
		let start = usize::try_from(section.file_offset()).expect("ELF file too large");
		let end = usize::try_from(section.file_offset() + section.mem_size()).expect("ELF file too large");
		&self.data.as_ref()[start..end]
	}
}

impl<D: AsRef<[u8]>> core::ops::Index<section::Section<'_, D>> for File<D> {
	type Output = [u8];

	fn index(&self, section: section::Section<'_, D>) -> &Self::Output {
		&self.inner[section]
	}
}

impl<D: AsRef<[u8]>> File<D> {
	pub fn load_with<E>(&self, mut f: impl FnMut(&segment::Segment, &[u8]) -> Result<(), E>) -> Result<(), E> {
		self.segments().filter(|segment| segment.ty() == segment::Type::LOAD).try_for_each(|segment| -> Result<(), E> {
			let data = &self[segment];
			f(&segment, data)
		})
	}
}

#[derive(Copy, Clone, Debug)]
pub struct Abi {
	pub os: OsAbi,
	pub version: u8,
}

newtype_enum! {
	pub enum OsAbi: pub u8 => {
		SYSTEM_V = 0,
		HP_UX = 1,
		NET_BSD = 2,
		GNU = 3,
		LINUX = Self::GNU.0,
		SOLARIS = 6,
		AIX = 7,
		IRIX = 8,
		FREE_BSD = 9,
		TRU64 = 10,
		MODESTO = 11,
		OPEN_BSD = 12,
		OPEN_VMS = 13,
		HP_NSK = 14,
		AMIGA = 15,
		FENIXOS = 16,
		CLOUD_ABI = 17,
		OPEN_VOS = 18,
		POPCORN = 200,
	}
}

newtype_enum! {
	pub enum Isa: pub u16 => {
		X86 = 0x03,
		X86_64 = 0x3E,
	}
}

newtype_enum! {
	pub enum Type: pub u16 => {
		RELOCATABLE = 1,
		EXECUTABLE = 2,
		SHARED = 3,
		CORE = 4,
	}
}

newtype_enum! {
	pub enum Width: pub u8 => {
		X32 = 1,
		X64 = 2,
	}
}

newtype_enum! {
	/// The endianness used to write headers.
	pub enum Endianness: pub u8 => {
		LITTLE = 1,
		BIG = 2,
	}
}
