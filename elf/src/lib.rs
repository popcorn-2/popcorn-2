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
pub mod note;
pub mod raw;

pub use header::file::Header as FileHeader;
pub use header::segment;
pub use header::section;

/// The error returned when parsing a [`File`].
#[derive(Debug, Clone, Copy)]
pub enum ParseError {
	/// An error was encountered parsing the [`FileHeader`].
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

/// A parsed ELF file.
#[derive(Debug, Clone, Copy)]
pub struct File<D> {
	inner: FileInner<D>,
}

#[derive(Debug, Clone, Copy)]
struct FileInner<D> {
	file_header: FileHeader,
	data: D,
}

impl<D> File<D> {
	/// The width of data structures in the ELF file.
	///
	/// Parsed from `e_ident[EI_CLASS]`.
	#[doc(alias = "EI_CLASS")]
	pub const fn width(&self) -> Width { self.inner.file_header.width() }

	/// The endianness used in headers.
	///
	/// Parsed from `e_ident[EI_DATA]`.
	#[doc(alias = "EI_DATA")]
	pub const fn endianness(&self) -> Endianness { self.inner.file_header.endianness() }

	/// The ABI the ELF file was compiled for.
	///
	/// Parsed from `e_ident[EI_OSABI]` and `e_ident[EI_ABIVERSION]`.
	#[doc(alias = "EI_OSABI")]
	#[doc(alias = "EI_ABIVERSION")]
	pub const fn abi(&self) -> Abi { self.inner.file_header.abi() }

	/// The architecture the ELF file was compiled for.
	///
	/// Parsed from `e_machine`.
	#[doc(alias = "e_machine")]
	pub const fn isa(&self) -> Isa { self.inner.file_header.isa() }

	/// The type of ELF file.
	///
	/// Parsed from `e_type`.
	#[doc(alias = "e_type")]
	pub const fn ty(&self) -> Type { self.inner.file_header.ty() }

	/// The virtual address of the entrypoint of the ELF file.
	///
	/// Parsed from `e_entry`.
	#[doc(alias = "e_entry")]
	pub fn entry_point(&self) -> usize { self.inner.file_header.entry_point() }
}

impl<D: AsRef<[u8]>> File<D> {
	/// Parses an ELF file from the raw data.
	///
	/// # Errors
	///
	/// Returns a [`ParseError`] if the file could not be parsed.
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

	/// Returns an iterator over each [`Segment`](segment::Segment) in the ELF file.
	pub fn segments(&self) -> segment::Iter<'_> {
		let data = self.inner.data.as_ref();
		let entries = &data[self.inner.file_header.program_header()];
		segment::Iter::new(
			entries,
			&self.inner.file_header,
		)
	}

	/// Returns an iterator over each [`Section`](section::Section) in the ELF file.
	pub fn sections(&self) -> section::Iter<'_, D> {
		section::Iter::new(
			&self.inner,
			self.inner.file_header.section_header(),
		)
	}

	/// Returns an iterator over each [`Note`](note::Note) object in the ELF file.
	///
	/// # Examples
	///
	/// Print the GNU build ID:
	///
	/// ```
	/// use elf::File;
	///
	/// # fn main() -> Result<(), elf::ParseError> {
    /// fn load_elf_file() -> &'static [u8] {
    /// #   return include_bytes!("../tests/build-id-x64.elf");
    ///     // ...
    /// }
    ///
    /// let file = File::try_new(load_elf_file())?;
    /// if let Some(note) = file.notes().find(|note|
    ///     note.name() == c"GNU"
    ///     && note.ty() == 3
    /// ) {
    /// #   // this weirdness is so we can doctest with a specific file without exposing the details of
    /// #   // it in the docs
    /// #   const TEST: [u8; 20] = [0x10, 0x57, 0xff, 0x44, 0x36, 0x9e, 0xd2, 0x50, 0xee, 0x2f, 0xe1, 0x97, 0x9e, 0x67, 0x36, 0x60, 0x6b, 0xd7, 0xaa, 0xda];
    /// #   let mut idx = 0;
    ///     print!("Build ID: ");
    ///     for byte in note.description() {
    ///         print!("{byte:x}");
    /// #       assert_eq!(*byte, TEST[idx]);
    /// #       idx += 1;
    ///     }
    ///     println!();
    /// }
    /// # else { panic!("test file contains a build ID") }
    /// # Ok(())
    /// # }
	/// ```
	pub fn notes(&self) -> impl Iterator<Item = note::Note<'_>> {
		self.sections()
			.filter(|section| section.ty() == section::Type::NOTE)
			.flat_map(|section| note::Iter::new(self.endianness(), &self[section]))
	}

	/// Iterates over the [loadable segments](segment::Type::LOAD) in the ELF file, calling the
	/// provided function with the parsed segment descriptor and segment content.
	///
	/// # Errors
	///
	/// Propagates any errors from the provided function.
	pub fn load_with<E>(&self, mut f: impl FnMut(&segment::Segment, &[u8]) -> Result<(), E>) -> Result<(), E> {
		self.segments().filter(|segment| segment.ty() == segment::Type::LOAD).try_for_each(|segment| -> Result<(), E> {
			let data = &self[segment];
			f(&segment, data)
		})
	}
}

impl<D: AsRef<[u8]>> FileInner<D> {
	/// Returns the section header at index `idx`.
	fn section_header(&self, idx: u32) -> &[u8] {
		#[expect(clippy::missing_panics_doc, reason = "allocations cannot be larger than isize::MAX so a file large enough to overflow usize cannot exist")]
		let idx = {
			let idx = usize::try_from(idx).expect("not supported on 16 bit");
			let start = idx * usize::from(self.file_header.section_header_entry_size());
			let end = (idx + 1) * usize::from(self.file_header.section_header_entry_size());
			start..end
		};

		&self.data.as_ref()[self.file_header.section_header()][idx]
	}
}

impl<D: AsRef<[u8]>> core::ops::Index<segment::Segment> for FileInner<D> {
	type Output = [u8];

	fn index(&self, index: segment::Segment) -> &Self::Output {
		let start = usize::try_from(index.file_offset()).expect("ELF file too large");
		let end = usize::try_from(index.file_offset() + index.file_size()).expect("ELF file too large");
		&self.data.as_ref()[start..end]
	}
}

impl<D: AsRef<[u8]>> core::ops::Index<segment::Segment> for File<D> {
	type Output = [u8];

	/// Returns the file content in the [`Segment`](segment::Segment) `index`.
	fn index(&self, index: segment::Segment) -> &Self::Output {
		&self.inner[index]
	}
}

impl<D: AsRef<[u8]>> core::ops::Index<section::Section<'_, D>> for FileInner<D> {
	type Output = [u8];

	fn index(&self, index: section::Section<'_, D>) -> &Self::Output {
		let start = usize::try_from(index.file_offset()).expect("ELF file too large");
		let end = usize::try_from(index.file_offset() + index.mem_size()).expect("ELF file too large");
		&self.data.as_ref()[start..end]
	}
}

impl<D: AsRef<[u8]>> core::ops::Index<section::Section<'_, D>> for File<D> {
	type Output = [u8];

	/// Returns the file content in the [`Section`](section::Section) `index`.
	fn index(&self, index: section::Section<'_, D>) -> &Self::Output {
		&self.inner[index]
	}
}

/// The ABI an ELF file was compiled for.
#[derive(Copy, Clone, Debug)]
pub struct Abi {
	/// The OS.
	pub os: OsAbi,
	/// The version of the ABI.
	pub version: u8,
}

newtype_enum! {
	/// The OS an ELF file is compiled for.
	pub enum OsAbi: pub u8 => {
		/// System V ABI.
		SYSTEM_V = 0,
		/// Hewlett-Packard HP-UX.
		HP_UX = 1,
		/// NetBSD.
		NET_BSD = 2,
		/// GNU.
		GNU = 3,
		/// Linux.
		LINUX = Self::GNU.0,
		/// Sun Solaris.
		SOLARIS = 6,
		/// AIX.
		AIX = 7,
		/// IRIX.
		IRIX = 8,
		/// FreeBSD.
		FREE_BSD = 9,
		/// Compaq TRU64 UNIX.
		TRU64 = 10,
		/// Novell Modesto.
		MODESTO = 11,
		/// Open BSD.
		OPEN_BSD = 12,
		/// Open VMS.
		OPEN_VMS = 13,
		/// Hewlett-Packard Non-Stop Kernel.
		HP_NSK = 14,
		/// Amiga OS.
		AMIGA = 15,
		/// FenixOS.
		FENIXOS = 16,
		/// Nuxi CloudABI.
		CLOUD_ABI = 17,
		/// Stratus Technologies OpenVOS.
		OPEN_VOS = 18,
		/// Popcorn2.
		POPCORN = 200,
	}
}

newtype_enum! {
	/// The architecture an ELF file is compiled for.
	pub enum Isa: pub u16 => {
		/// Intel x86.
		X86 = 0x03,
		/// Intel x86_64.
		X86_64 = 0x3E,
	}
}

newtype_enum! {
	/// The type of ELF file.
	pub enum Type: pub u16 => {
		/// Relocatable object file.
		RELOCATABLE = 1,
		/// Executable file.
		EXECUTABLE = 2,
		/// Dynamically linked object file.
		SHARED = 3,
		/// Core dump.
		CORE = 4,
	}
}

newtype_enum! {
	/// The data size used in headers.
	pub enum Width: pub u8 => {
		/// 32-bit ELF file.
		X32 = 1,
		/// 64-bit ELF file.
		X64 = 2,
	}
}

newtype_enum! {
	/// The endianness used for the content of headers.
	pub enum Endianness: pub u8 => {
		/// Little endian, 2's complement encoding.
		LITTLE = 1,
		/// Big endian, 2's complement encoding.
		BIG = 2,
	}
}
