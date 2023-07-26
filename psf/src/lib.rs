#![feature(pointer_byte_offsets)]
#![feature(result_option_inspect)]
#![feature(pointer_is_aligned)]
#![cfg_attr(not(feature = "std"), no_std)]

use core::fmt;
use core::mem::{size_of, align_of};
use core::slice;
use core::fmt::Write;
use core::mem;
use bitflags::{bitflags, Flags};
use derive_more::Display;

#[derive(Debug)]
#[repr(C)]
struct Psf1 {
    magic: [u8; 2],
    mode: FontMode,
    char_size: u8
}

#[repr(C)]
struct Psf2 {
    magic: [u8; 4],
    version: u32,
    header_size: u32,
    flags: FontFlags,
    glyph_count: u32,
    char_size: u32,
    glyph_height: u32,
    glyph_width: u32
}

bitflags! {
    #[derive(Debug)]
	struct FontMode: u8 {
		const _512 = 1;
		const HashTable = 2;
		const Seq = 4;
	}

    #[derive(Debug)]
	struct FontFlags: u32 {
		const UnicodeTable = 1;
	}
}

pub struct PsfChar<'a> {
    width: usize,
    height: usize,
    stride: usize,
    data: &'a [u8]
}

impl<'a> PsfChar<'a> {
    pub fn is_set(&self, x: usize, y: usize) -> bool {
        if x > self.width || y > self.height { return false; }
        let (x_byte, x_bit) = (x / 8, x % 8);
        let data = self.data[x_byte + y * self.stride];

        (data & (1 << (7 - x_bit))) != 0
    }
}

impl<'a> fmt::Debug for PsfChar<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Glyph[")?;
        if !f.alternate() { f.write_str(" ")?; }
        for y in 0..self.height {
            if f.alternate() { f.write_str("\n\t")?; } else { f.write_str("0b")?; }
            for x in 0..self.width {
                match (self.is_set(x, y), f.alternate()) {
                    (true, true) => f.write_str("#")?,
                    (false, true) => f.write_str(" ")?,
                    (true, false) => f.write_str("1")?,
                    (false, false) => f.write_str("0")?,
                }
            }
            if !f.alternate() && y != (self.height - 1) { f.write_str(", ")?; }
        }
        if f.alternate() { f.write_str("\n")?; } else { f.write_str(" ")?; }
        f.write_str("]")?;
        Ok(())
    }
}

pub trait PsfFont {
    fn char_width(&self) -> usize;
    fn char_height(&self) -> usize;
    fn char_count(&self) -> usize;
    fn char_stride(&self) -> usize;
    fn header_size(&self) -> usize;

    fn locate_char<'s>(&'s self, the_char: char) -> Result<PsfChar<'s>, LookupError> {
        let the_char: usize = u32::from(the_char).try_into().unwrap();
        if the_char > 512 { return Err(LookupError::UnicodeChar); }
        if the_char > self.char_count() { return Err(LookupError::ExtendedGlyph); }

        let char_byte_offset = self.header_size() + (self.char_height() * self.char_stride() * the_char);
        let char_size = self.char_height() * self.char_stride();

        /* SAFETY:
         * Lifetime of data in font same as lifetime of font itself
         * Size of font data checked when creating Psf object
         */
        let char_data = unsafe {
            slice::from_raw_parts::<'s>((self as *const _ as *const u8).byte_add(char_byte_offset), char_size)
        };

        Ok(PsfChar {
            width: self.char_width(),
            height: self.char_height(),
            stride: self.char_stride(),
            data: char_data,
        })
    }
}

impl Psf1 {
    pub fn try_new(raw_font_data: &[u8]) -> Result<&Psf1, ParseError> {
        if raw_font_data.len() < size_of::<Psf1>() {
            return Err(ParseError::NoHeader);
        }
        if !raw_font_data.as_ptr().is_aligned_to(align_of::<Psf1>()) {
            return Err(ParseError::IncorrectAlign);
        }

        let font_data = unsafe {
            &*(raw_font_data as *const _ as *const Psf1)
        };

        if font_data.magic != [0x36, 0x04] {
            return Err(ParseError::IncorrectMagic);
        }

        if font_data.mode.contains(FontMode::Seq) || font_data.mode.contains(FontMode::HashTable) {
            return Err(ParseError::UnicodeTableUnsupported);
        }

        let expected_bytes = size_of::<Psf1>() + font_data.char_count() * usize::from(font_data.char_size);

        if expected_bytes != raw_font_data.len() {
            return Err(ParseError::IncorrectDataLength(expected_bytes, raw_font_data.len()));
        }

        Ok(font_data)
    }
}

impl PsfFont for Psf1 {
    fn char_width(&self) -> usize {
        8
    }

    fn char_height(&self) -> usize {
        self.char_size.into()
    }

    fn char_count(&self) -> usize {
        if self.mode.contains(FontMode::_512) { 512 } else { 256 }
    }

    fn char_stride(&self) -> usize {
        8
    }

    fn header_size(&self) -> usize {
        mem::size_of::<Self>()
    }
}

impl Psf2 {
    pub fn try_new(raw_font_data: &[u8]) -> Result<&Psf2, ParseError> {
        if raw_font_data.len() < size_of::<Psf2>() {
            return Err(ParseError::NoHeader);
        }
        if !raw_font_data.as_ptr().is_aligned_to(align_of::<Psf2>()) {
            return Err(ParseError::IncorrectAlign);
        }

        let font_data = unsafe {
            &*(raw_font_data as *const _ as *const Psf2)
        };

        if font_data.magic != [0x72, 0xb5, 0x4a, 0x86] {
            return Err(ParseError::IncorrectMagic);
        }

        if font_data.flags.contains(FontFlags::UnicodeTable) {
            return Err(ParseError::UnicodeTableUnsupported);
        }

        let expected_bytes = size_of::<Psf2>() + font_data.char_count() * usize::try_from(font_data.char_size).unwrap();

        if expected_bytes != raw_font_data.len() {
            return Err(ParseError::IncorrectDataLength(expected_bytes, raw_font_data.len()));
        }

        Ok(font_data)
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Display)]
pub enum ParseError {
    #[display(fmt = "Incorrect header alignment")]
    IncorrectAlign,
    #[display(fmt = "Invalid header")]
    IncorrectMagic,
    #[display(fmt = "Glyph data is incorrect size - Expected at least {_0} bytes, found {_1}")]
    IncorrectDataLength(usize, usize),
    #[display(fmt = "Font uses a currently unsupported Unicode table")]
    UnicodeTableUnsupported,
    #[display(fmt = "Font contains no header")]
    NoHeader,
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Display)]
pub enum LookupError {
    #[display(fmt = "Requested character is outside of supported ASCII range")]
    UnicodeChar,
    #[display(fmt = "Requested character outside of range this font supports")]
    ExtendedGlyph
}

impl PsfFont for Psf2 {
    fn char_width(&self) -> usize {
        self.glyph_width.try_into().unwrap()
    }

    fn char_height(&self) -> usize {
        self.glyph_height.try_into().unwrap()
    }

    fn char_count(&self) -> usize {
        self.glyph_count.try_into().unwrap()
    }

    fn char_stride(&self) -> usize {
        (self.char_size / self.glyph_height).try_into().unwrap()
    }

    fn header_size(&self) -> usize {
        mem::size_of::<Self>()
    }
}

pub fn try_parse(data: &[u8]) -> Result<&dyn PsfFont, ParseError> {
    match Psf1::try_new(data) {
        Ok(font) => Ok(font),
        Err(ParseError::IncorrectMagic) => Ok(Psf2::try_new(data)?),
        Err(e) => Err(e)
    }
}

#[cfg(test)]
mod tests {
    use crate::ParseError::IncorrectMagic;
    use crate::{Psf1, Psf2, PsfFont};

    static PSF_DATA_2: &'static [u8] = include_bytes!("font.psf2");
    static PSF_DATA_1: &'static [u8] = include_bytes!("font.psf1");

    #[test]
    fn parse_psf2_as_psf1() {
        let result = Psf1::try_new(PSF_DATA_2);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), IncorrectMagic);
    }

    #[test]
    fn parse_psf2() {
        let result = Psf2::try_new(PSF_DATA_2);
        result.unwrap();
    }

    #[test]
    fn parse_psf1() {
        let result = Psf1::try_new(PSF_DATA_1);
        result.unwrap();
    }

    #[test]
    fn print_char() {
        let result = Psf2::try_new(PSF_DATA_2).unwrap();
        let c = result.locate_char('A').unwrap();
        println!("{c:#?}");
        let c = result.locate_char('g').unwrap();
        println!("{c:?}");
    }
}
