use crate::raw::index_part;
use crate::{raw, FileInner, Width};
use bitflags::bitflags;
use core::ffi::CStr;
use core::fmt;
use core::ops::Range;

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

    pub fn name(&self) -> &CStr {
        let strtab = {
            let strtab = self.file.section_header(self.file.file_header.section_string_table().into());
            let strtab = Self::new(self.file, strtab);
            &self.file[strtab]
        };
        CStr::from_bytes_until_nul(&strtab[self.name_idx as usize..]).unwrap_or(c"")
    }

    pub fn link(&self) -> Option<Self> {
        if self.link == 0 { return None; }
        let link = self.file.section_header(self.link);
        Some(Self::new(
            self.file,
            link,
        ))
    }

    pub const fn ty(&self) -> Type {
        self.ty
    }

    pub const fn flags(&self) -> Flags {
        self.flags
    }

    pub const fn vaddr(&self) -> u64 {
        self.vaddr
    }

    pub(crate) const fn file_offset(&self) -> u64 {
        self.file_offset
    }

    pub const fn mem_size(&self) -> u64 {
        self.mem_size
    }

    pub const fn align(&self) -> u64 {
        self.align
    }

    /*pub fn link(&self) -> Option<Self> {
        let idx = self.link as usize;
        self.file.sections().nth(idx)
    }*/
}

#[derive(Debug)]
pub struct Iter<'f, D> {
    file: &'f FileInner<D>,
    entries: &'f [u8],
    base: u64,
}

impl<'f, D: AsRef<[u8]>> Iter<'f, D> {
    pub(crate) fn new(file: &'f FileInner<D>, base: u64, entries: Range<usize>) -> Self {
        Self {
            file,
            entries: &file.data.as_ref()[entries],
            base,
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
	/// Type of section in program header table.
	pub enum Type: u32 => {
		/// Unused entry.
		NULL = 0x00,
		/// Segment to be loaded.
		PROGRAM_BITS = 0x01,
		/// Dynamic linking table.
		SYMBOL_TABLE = 0x02,
		/// Interpreter path.
		STRING_TABLE = 0x03,
		/// Auxiliary data.
		RELA = 0x04,
        HASH = 0x05,
		/// Program header table.
		DYNAMIC = 0x06,
		/// TLS template data.
		NOTE = 0x07,
        NO_BITS = 0x08,
        REL = 0x09,
        DYNAMIC_SYMBOL_TABLE = 0x0B,
        INIT_ARRAY = 0x0E,
        FINI_ARRAY = 0x0F,
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
    /// Create a new `SectionType` with the given value.
    ///
    /// If the value falls outside the OS specific reserved values, returns `None`.
    pub fn new_os(value: u32) -> Option<Self> {
        if (Self::OS_LOW.0..=Self::OS_HIGH.0).contains(&value) { Some(Self(value)) }
        else { None }
    }

    /// Create a new `SectionType` with the given value.
    ///
    /// If the value falls outside the architecture specific reserved values, returns `None`.
    pub fn new_processor(value: u32) -> Option<Self> {
        if (Self::PROCESSOR_LOW.0..=Self::PROCESSOR_HIGH.0).contains(&value) { Some(Self(value)) }
        else { None }
    }
}

bitflags! {
	/// Flags describing how to load a section.
	#[derive(Debug, Copy, Clone)]
	#[repr(C)]
	pub struct Flags: u64 {
		/// Executable code.
		const Write = 0x1;
		/// Writeable data.
		const Alloc = 0x2;
		/// Readable data.
        const Exec = 0x4;
        const Merge = 0x10;
        const Strings = 0x20;
        const Tls = 0x400;
		const OsMask = 0x0F000000;
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
