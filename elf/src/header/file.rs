use core::{fmt, mem};
use core::mem::size_of;
use core::ops::Range;
use debug_stub_derive::DebugStub;
use derive_more::Display;
use num_enum::{TryFromPrimitiveError, TryFromPrimitive};

#[derive(DebugStub)]
#[repr(C)]
pub struct FileHeader {
	magic: [u8; 4],
	pub width: Width,
	pub endianness: Endianness,
	pub header_version: u8,
	pub abi: Abi,
	padding: u64,
	pub file_type: Type,
	pub isa: Isa,
	pub elf_version: u32,
	#[debug_stub = "..."]
	extra: ExtraHeader
}

impl FileHeader {
	pub fn entry_point(&self) -> usize {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.entry_point.try_into().unwrap() },
			Width::_64 => unsafe { self.extra._64.entry_point.try_into().unwrap() }
		}
	}

	pub fn program_header(&self) -> Range<usize> {
		let start = self.program_header_offset();
		let size = self.program_header_entry_size() * self.program_header_entry_count();

		Range{start, end: start + size}
	}

	pub fn section_header(&self) -> Range<usize> {
		let start = self.section_header_offset();
		let size = self.section_header_entry_size() * self.section_header_entry_count();

		Range{start, end: start + size}
	}

	fn program_header_offset(&self) -> usize {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.program_header_offset.try_into().unwrap() },
			Width::_64 => unsafe { self.extra._64.program_header_offset.try_into().unwrap() }
		}
	}

	fn section_header_offset(&self) -> usize {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.section_header_offset.try_into().unwrap() },
			Width::_64 => unsafe { self.extra._64.section_header_offset.try_into().unwrap() }
		}
	}

	pub fn flags(&self) -> u32 {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.flags },
			Width::_64 => unsafe { self.extra._64.flags }
		}
	}

	pub fn header_size(&self) -> usize {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.header_size },
			Width::_64 => unsafe { self.extra._64.header_size }
		}.try_into().unwrap()
	}

	pub(crate) fn program_header_entry_size(&self) -> usize {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.program_header_entry_size },
			Width::_64 => unsafe { self.extra._64.program_header_entry_size }
		}.try_into().unwrap()
	}

	pub(crate) fn section_header_entry_size(&self) -> usize {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.section_header_entry_size },
			Width::_64 => unsafe { self.extra._64.section_header_entry_size }
		}.try_into().unwrap()
	}

	fn program_header_entry_count(&self) -> usize {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.program_header_entry_count },
			Width::_64 => unsafe { self.extra._64.program_header_entry_count }
		}.try_into().unwrap()
	}

	fn section_header_entry_count(&self) -> usize {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.section_header_entry_count },
			Width::_64 => unsafe { self.extra._64.section_header_entry_count }
		}.try_into().unwrap()
	}

	pub fn string_table_index(&self) -> u16 {
		match self.width {
			// SAFETY: Union state based on width field
			Width::_32 => unsafe { self.extra._32.string_table_index },
			Width::_64 => unsafe { self.extra._64.string_table_index }
		}
	}
}

impl<'a> TryFrom<&'a FileHeaderRaw> for &'a FileHeader {
	type Error = Error;

	fn try_from(value: &'a FileHeaderRaw) -> Result<&'a FileHeader, Self::Error> {
		Width::try_from(value.arch_width)?;
		Endianness::try_from(value.endianness)?;
		Abi::try_from(value.abi)?;
		Type::try_from(value.file_type)?;
		Isa::try_from(value.isa)?;

		// SAFETY: Checked each enum has valid value
		Ok(unsafe { mem::transmute::<_, &'a FileHeader>(value) })
	}
}

/*
impl fmt::Debug for FileHeader {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("FileHeader")
		 .field("magic", &self.magic)
		 .field("arch_width", &self.width)
		 .field("endianness", &self.endianness)
		 .field("header_version", &self.header_version)
		 .field("abi", &self.abi)
		 .field("file_type", &self.file_type)
		 .field("isa", &self.isa)
		 .field("elf_version", &self.elf_version)
		 .field("entry_point", &self.entry_point())
		 .field("program_header_offset", &self.program_header_offset())
		 .field("section_header_offset", &self.section_header_offset())
		 .field("flags", &self.flags())
		 .field("header_size", &self.header_size())
		 .field("program_header_entry_size", &self.program_header_entry_size())
		 .field("section_header_entry_size", &self.section_header_entry_size())
		 .field("program_header_entry_count", &self.program_header_entry_count())
		 .field("section_header_entry_count", &self.section_header_entry_count())
		 .field("string_table_index", &self.string_table_index())
		 .finish()
	}
}
*/
#[repr(C)]
union ExtraHeader {
	pub _32: FileHeaderBit<u32>,
	pub _64: FileHeaderBit<u64>
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct FileHeaderBit<T> {
	pub entry_point: T,
	pub program_header_offset: T,
	pub section_header_offset: T,
	pub flags: u32,
	pub header_size: u16,
	pub program_header_entry_size: u16,
	pub program_header_entry_count: u16,
	pub section_header_entry_size: u16,
	pub section_header_entry_count: u16,
	pub string_table_index: u16
}

#[repr(C)]
pub struct FileHeaderRaw {
	magic: [u8; 4],
	pub arch_width: u8,
	pub endianness: u8,
	pub header_version: u8,
	pub abi: u8,
	padding: u64,
	pub file_type: u16,
	pub isa: u16,
	pub elf_version: u32,
	extra: ExtraHeader
}

impl fmt::Debug for FileHeaderRaw {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("FileHeaderRaw")
		 .field("magic", &self.magic)
		 .field("arch_width", &self.arch_width)
		 .field("endianness", &self.endianness)
		 .field("header_version", &self.header_version)
		 .field("abi", &self.abi)
		 .field("file_type", &self.file_type)
		 .field("isa", &self.isa)
		 .field("elf_version", &self.elf_version)
		 .field("rest", &"..")
		 .finish()
	}
}

impl FileHeader {
	pub(crate) fn try_new(data: &[u8]) -> Result<&FileHeader, Error> {
		if data.len() < size_of::<FileHeaderRaw>() {
			return Err(Error::NoHeader);
		}
		let header_ptr = data.as_ptr().cast::<FileHeaderRaw>();
		if !header_ptr.is_aligned() {
			return Err(Error::IncorrectAlign);
		}

		// SAFETY: Checked alignment, and non-null since taken from slice
		let data = unsafe {
			&*header_ptr
		};

		let data = <&FileHeader>::try_from(data)?;

		if data.magic != [0x7f, b'E', b'L', b'F'] {
			return Err(Error::WrongMagic);
		}

		Ok(data)
	}
}

#[derive(Debug, Display)]
pub enum Error {
	#[display(fmt = "File too short")]
	NoHeader,
	#[display(fmt = "Data not aligned - expected alignment of {}", "mem::align_of::<FileHeader>()")]
	IncorrectAlign,
	#[display(fmt = "Corrupted file")]
	WrongMagic,
	InvalidWidth(<Width as TryFromPrimitive>::Primitive),
	InvalidEndianness(<Endianness as TryFromPrimitive>::Primitive),
	InvalidAbi(<Abi as TryFromPrimitive>::Primitive),
	InvalidType(<Type as TryFromPrimitive>::Primitive),
	InvalidIsa(<Isa as TryFromPrimitive>::Primitive),
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum Width {
	_32 = 1,
	_64 = 2
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum Endianness {
	Little = 1,
	Big = 2
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum Abi {
	SystemV = 0
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum Type {
	Relocatable = 1,
	Executable = 2,
	Shared = 3,
	Core = 4
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u16)]
pub enum Isa {
	Nonspecific = 0,
	Sparc = 2,
	X86 = 3,
	Mips = 8,
	PowerPc = 0x14,
	Arm = 0x28,
	SuperH = 0x2a,
	Itanium = 0x32,
	Amd64 = 0x3e,
	AArch64 = 0xb7,
	RiscV = 0xf3
}

impl From<TryFromPrimitiveError<Width>> for Error {
	fn from(value: TryFromPrimitiveError<Width>) -> Self {
		Self::InvalidWidth(value.number)
	}
}

impl From<TryFromPrimitiveError<Endianness>> for Error {
	fn from(value: TryFromPrimitiveError<Endianness>) -> Self {
		Self::InvalidEndianness(value.number)
	}
}

impl From<TryFromPrimitiveError<Abi>> for Error {
	fn from(value: TryFromPrimitiveError<Abi>) -> Self {
		Self::InvalidAbi(value.number)
	}
}

impl From<TryFromPrimitiveError<Type>> for Error {
	fn from(value: TryFromPrimitiveError<Type>) -> Self {
		Self::InvalidType(value.number)
	}
}

impl From<TryFromPrimitiveError<Isa>> for Error {
	fn from(value: TryFromPrimitiveError<Isa>) -> Self {
		Self::InvalidIsa(value.number)
	}
}
