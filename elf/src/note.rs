//! Types for reading ELF note sections.

use core::ffi::CStr;
use crate::Endianness;

/// A parsed ELF note entry.
#[derive(Clone, Copy, Debug)]
pub struct Note<'f> {
    name: &'f CStr,
    description: &'f [u8],
    ty: u32,
}

impl Note<'_> {
    /// The name of this note.
    #[must_use]
    pub const fn name(&self) -> &CStr { self.name }

    /// The content of this note.
    ///
    /// Interpretation of this is specific to the name and type.
    #[must_use]
    pub const fn description(&self) -> &[u8] { self.description }

    /// The type of the note.
    #[must_use]
    pub const fn ty(&self) -> u32 { self.ty }
}

/// An iterator of each [`Note`] in a note section.
#[derive(Debug)]
pub struct Iter<'f> {
    endianness: Endianness,
    entries: &'f [u8],
}

impl<'f> Iter<'f> {
    pub(crate) const fn new(endianness: Endianness, data: &'f [u8]) -> Self {
        Self {
            endianness,
            entries: data,
        }
    }
}

impl<'f> Iterator for Iter<'f> {
    type Item = Note<'f>;

    fn next(&mut self) -> Option<Note<'f>> {
        let name_size: [u8; 4] = self.entries.get(..4)?.try_into().expect("indexed 4 bytes");
        let desc_size: [u8; 4] = self.entries.get(4..8)?.try_into().expect("indexed 4 bytes");
        let ty: [u8; 4] = self.entries.get(8..12)?.try_into().expect("indexed 4 bytes");

        let name_size: usize = self.endianness.decode_u32(name_size).try_into().expect("16 bit arch not supported");
        let desc_size: usize = self.endianness.decode_u32(desc_size).try_into().expect("16 bit arch not supported");
        let ty = self.endianness.decode_u32(ty);

        let (entry, rest) = {
            let post_name = 12 + name_size;
            let post_name_aligned = 4*post_name.div_ceil(4);
            let post_desc = post_name_aligned + desc_size;
            let post_desc_aligned = 4*post_desc.div_ceil(4);
            self.entries.split_at_checked(post_desc_aligned)?
        };
        self.entries = rest;

        let name = {
            let start = 12;
            let end = start + name_size;
            CStr::from_bytes_until_nul(&entry[start..end]).expect("malformed note")
        };

        let description = {
            let start = 4*(12 + name_size).div_ceil(4);
            let end = start + desc_size;
            &entry[start..end]
        };

        Some(Note {
            name,
            description,
            ty,
        })
    }
}
