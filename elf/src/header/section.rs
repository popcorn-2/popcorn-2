//! Types for reading ELF section headers.

use crate::raw::index_part;
use crate::{raw, FileInner, Width};
use bitflags::bitflags;
use core::ffi::CStr;
use core::fmt;
use core::ops::Range;

/// A parsed ELF section header entry.
pub struct Section<'f, D> {
    file: &'f FileInner<D>,
    name_idx: u32,
    ty: Type,
    flags: Flags,
    vaddr: u64,
    file_offset: u64,
    mem_size: u64,
    align: u64,
    link: u32,
    info: u32,
}

impl<D> Clone for Section<'_, D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D> Copy for Section<'_, D> {}

impl<D: AsRef<[u8]>> fmt::Debug for Section<'_, D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Section")
            .field("name", &self.name())
            .field("ty", &self.ty)
            .field("flags", &self.flags)
            .field("vaddr", &self.vaddr)
            .field("file_offset", &self.file_offset)
            .field("mem_size", &self.mem_size)
            .field("align", &self.align)
            .field("link", &self.link)
            .field("info", &self.info)
            .finish()
    }
}

impl<'f, D: AsRef<[u8]>> Section<'f, D> {
    pub(crate) fn new(file: &'f FileInner<D>, entry: &'f [u8]) -> Self {
        let width = file.file_header.width();
        let endianness = file.file_header.endianness();
        match width {
            Width::X32 => {
                let name_idx = endianness.decode_u32(index_part::<raw::section::x32::Name>(entry));
                let ty = Type(endianness.decode_u32(index_part::<raw::section::x32::Type>(entry)));
                let flags = Flags::from_bits_retain(u64::from(endianness.decode_u32(index_part::<raw::section::x32::Flags>(entry))));
                let vaddr = endianness.decode_u32(index_part::<raw::section::x32::VAddr>(entry));
                let offset = endianness.decode_u32(index_part::<raw::section::x32::Offset>(entry));
                let size = endianness.decode_u32(index_part::<raw::section::x32::Size>(entry));
                let link = endianness.decode_u32(index_part::<raw::section::x32::Link>(entry));
                let info = endianness.decode_u32(index_part::<raw::section::x32::Info>(entry));
                let align = endianness.decode_u32(index_part::<raw::section::x32::Align>(entry));
                let _entry_size = endianness.decode_u32(index_part::<raw::section::x32::EntrySize>(entry));

                Section {
                    file,
                    name_idx,
                    ty,
                    flags,
                    vaddr: u64::from(vaddr),
                    file_offset: u64::from(offset),
                    mem_size: u64::from(size),
                    align: u64::from(align),
                    link,
                    info,
                }
            },
            Width::X64 => {
                let name_idx = endianness.decode_u32(index_part::<raw::section::x64::Name>(entry));
                let ty = Type(endianness.decode_u32(index_part::<raw::section::x64::Type>(entry)));
                let flags = Flags::from_bits_retain(endianness.decode_u64(index_part::<raw::section::x64::Flags>(entry)));
                let vaddr = endianness.decode_u64(index_part::<raw::section::x64::VAddr>(entry));
                let offset = endianness.decode_u64(index_part::<raw::section::x64::Offset>(entry));
                let size = endianness.decode_u64(index_part::<raw::section::x64::Size>(entry));
                let link = endianness.decode_u32(index_part::<raw::section::x64::Link>(entry));
                let info = endianness.decode_u32(index_part::<raw::section::x64::Info>(entry));
                let align = endianness.decode_u64(index_part::<raw::section::x64::Align>(entry));
                let _entry_size = endianness.decode_u64(index_part::<raw::section::x64::EntrySize>(entry));

                Section {
                    file,
                    name_idx,
                    ty,
                    flags,
                    vaddr,
                    file_offset: offset,
                    mem_size: size,
                    align,
                    link,
                    info,
                }
            },
            _ => unimplemented!("unknown architecture width"),
        }
    }

    /// The name of this section.
    pub fn name(&self) -> &CStr {
        let strtab = {
            let strtab = self.file.section_header(self.file.file_header.section_string_table().into());
            let strtab = Self::new(self.file, strtab);
            &self.file[strtab]
        };
        CStr::from_bytes_until_nul(&strtab[self.name_idx as usize..]).unwrap_or(c"")
    }

    /// Returns the section specified in the `sh_link` field if it exists.
    pub fn link(&self) -> Option<Self> {
        if self.link == 0 { return None; }
        let link = self.file.section_header(self.link);
        Some(Self::new(
            self.file,
            link,
        ))
    }

    /// The type of this section.
    pub const fn ty(&self) -> Type {
        self.ty
    }

    /// Additional information about the section contents.
    pub const fn flags(&self) -> Flags {
        self.flags
    }

    /// The virtual address this section should be loaded at.
    pub const fn vaddr(&self) -> u64 {
        self.vaddr
    }

    pub(crate) const fn file_offset(&self) -> u64 {
        self.file_offset
    }

    /// Size this section takes up once loaded into memory.
    ///
    /// > **Note**: This may be larger than the length of the data contained
    /// > in this section. The remaining space should be filled with zeroes.
    pub const fn mem_size(&self) -> u64 {
        self.mem_size
    }

    /// Required alignment of the start of this section.
    pub const fn align(&self) -> u64 {
        self.align
    }
}

/// An iterator of each [`Section`] in a file.
#[derive(Debug)]
pub struct Iter<'f, D> {
    file: &'f FileInner<D>,
    entries: &'f [u8],
}

impl<'f, D: AsRef<[u8]>> Iter<'f, D> {
    pub(crate) fn new(file: &'f FileInner<D>, entries: Range<usize>) -> Self {
        Self {
            file,
            entries: &file.data.as_ref()[entries],
        }
    }
}

impl<'f, D: AsRef<[u8]>> Iterator for Iter<'f, D> {
    type Item = Section<'f, D>;

    fn next(&mut self) -> Option<Self::Item> {
        let (entry, rest) = self.entries.split_at_checked(self.file.file_header.section_header_entry_size().into())?;
        self.entries = rest;

        match self.file.file_header.width() {
            Width::X32 if entry.len() < raw::section::x32::SIZE => return None,
            Width::X64 if entry.len() < raw::section::x64::SIZE => return None,
            Width::X32 | Width::X64 => {},
            _ => unimplemented!("malformed ELF file"),
        }

        Some(Section::new(self.file, entry))
    }
}

kernel_api::newtype_enum! {
	/// Type of section.
	pub enum Type: u32 => {
		/// Unused entry.
		NULL = 0x00,
		/// Contains program data.
		PROGRAM_BITS = 0x01,
		/// Contains symbol table.
		SYMBOL_TABLE = 0x02,
        /// Contains string table.
		STRING_TABLE = 0x03,
		/// List of relocations with addends.
		RELA = 0x04,
        /// Hashtable of symbols.
        HASH = 0x05,
		/// Dynamic linking table.
		DYNAMIC = 0x06,
        /// Notes.
		NOTE = 0x07,
        /// Reserved data space with no content.
        NO_BITS = 0x08,
        /// List of relocations.
        REL = 0x09,
        /// Symbol table for dynamic linking.
        DYNAMIC_SYMBOL_TABLE = 0x0B,
        /// Array of constructor addresses.
        INIT_ARRAY = 0x0E,
        /// Array of destructor addresses.
        FINI_ARRAY = 0x0F,
        /// Array of pre-constructor addresses.
        PRE_INIT_ARRAY = 0x10,
		/// Lowest value reserved for OS use.
		OS_LOW = 0x6000_0000,
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
    pub fn new_os(value: u32) -> Option<Self> {
        if (Self::OS_LOW.0..=Self::OS_HIGH.0).contains(&value) { Some(Self(value)) }
        else { None }
    }

    /// Create a new `Type` with the given value.
    ///
    /// If the value falls outside the architecture specific reserved values, returns `None`.
    pub fn new_processor(value: u32) -> Option<Self> {
        if (Self::PROCESSOR_LOW.0..=Self::PROCESSOR_HIGH.0).contains(&value) { Some(Self(value)) }
        else { None }
    }
}

bitflags! {
	/// Flags containing additional information about the contents of a section.
	#[derive(Debug, Copy, Clone)]
	#[repr(C)]
	pub struct Flags: u64 {
		/// Executable code.
		const Write = 0x1;
		/// Writeable data.
		const Alloc = 0x2;
		/// Readable data.
        const Exec = 0x4;
        /// Can be merged with other sections.
        const Merge = 0x10;
        /// Contains strings.
        const Strings = 0x20;
        /// Contains thread local template.
        const Tls = 0x400;
        /// OS specific flags.
		const OsMask = 0x0F000000;
        /// Architecture specific flags.
		const ProcessorMask = 0xF0000000;
	}
}

impl Flags {
    /// Create a new `SectionFlags` with the given value.
    ///
    /// If the value falls outside the OS specific reserved values, returns `None`.
    pub fn new_os(value: u64) -> Option<Self> {
        let masked = value & Self::OsMask.bits();
        if masked == value { Some(Self::from_bits_retain(value)) }
        else { None }
    }

    /// Create a new `SectionFlags` with the given value.
    ///
    /// If the value falls outside the architecture specific reserved values, returns `None`.
    pub fn new_processor(value: u64) -> Option<Self> {
        let masked = value & Self::ProcessorMask.bits();
        if masked == value { Some(Self::from_bits_retain(value)) }
        else { None }
    }
}
