#![no_std]

mod pixel;
mod header;
mod errors;
pub use pixel::*;

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "alloc")]
use alloc::borrow;

use core::mem;
use crate::errors::ParsingError;

#[derive(Debug)]
pub struct Image<'a> {
    #[cfg(feature = "alloc")]
    pub pixel_data: borrow::Cow<'a, [u8]>,
    #[cfg(not(feature = "alloc"))]
    pub pixel_data: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub color_format: PixelFormat,
    pub ordering: (PixelOrderVertical, PixelOrderHorizontal)
}

#[derive(Debug)]
pub enum PixelOrderVertical {
    TopToBottom,
    BottomToTop,
}

#[derive(Debug)]
pub enum PixelOrderHorizontal {
    LeftToRight,
    RightToLeft,
}

#[derive(Debug)]
pub enum PixelFormat {
    Rgb555,
    Rgba5551,
    Rgb888,
    Rgba8888
}

impl PixelFormat {
    fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelFormat::Rgb555 => 2,
            PixelFormat::Rgba5551 => 2,
            PixelFormat::Rgb888 => 3,
            PixelFormat::Rgba8888 => 4,
        }
    }
}

impl<'a> Image<'a> {
    pub fn try_new(data_raw: &'a [u8]) -> Result<Self, ParsingError> {
        if data_raw.len() < mem::size_of::<header::Header>() { return Err(ParsingError::NoHeader); }

        // SAFETY: All fields are integers and data is long enough
        // Alignment?
        let data = unsafe { &*(data_raw.as_ptr() as *const header::Header) };
        if data.has_color_map() { return Err(ParsingError::Unsupported); }

        match data.image_format() {
            header::Type::UncompressedTrueColor => {}
            _ => return Err(ParsingError::Unsupported)
        }

        let format = data.color_format()?;

        let ordering = data.pixel_ordering();

        let expected_data_size = format.bytes_per_pixel() * data.width() * data.height();

        if (expected_data_size + mem::size_of::<header::Header>()) > data_raw.len() { return Err(ParsingError::NotEnoughData); }

        Ok(Image {
            pixel_data: data_raw[mem::size_of::<header::Header>()..].into(),
            width: data.width(),
            height: data.height(),
            color_format: format,
            ordering,
        })
    }
}
