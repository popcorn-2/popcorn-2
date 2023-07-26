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

impl<'a> Image<'a> {
    pub fn get_pixel(&self, mut x: usize, mut y: usize) -> Option<Pixel> {
        if x >= self.width || y >= self.height { return None; }

        match self.ordering.0 {
            PixelOrderVertical::TopToBottom => {}
            PixelOrderVertical::BottomToTop => y = self.height - y - 1
        }
        match self.ordering.1 {
            PixelOrderHorizontal::LeftToRight => {}
            PixelOrderHorizontal::RightToLeft => x = self.width - x - 1
        }

        let pixel_offset = y * self.width + x;
        let byte_offset = pixel_offset * self.color_format.bytes_per_pixel();

        // SAFETY: Bounds checked on entry to function
        let raw_data = unsafe { self.pixel_data.get_unchecked(byte_offset..byte_offset+self.color_format.bytes_per_pixel()) };

        let pixel = match self.color_format {
            PixelFormat::Rgb555 => todo!("Rgb555 format currently not supported"),
            PixelFormat::Rgba5551 => todo!("Rgb5551 format currently not supported"),
            PixelFormat::Rgb888 => Pixel{ r: raw_data[2], g: raw_data[1], b: raw_data[0], a: 255 },
            PixelFormat::Rgba8888 => Pixel{ r:raw_data[2], g: raw_data[1], b: raw_data[0], a: raw_data[3] },
        };
        Some(pixel)
    }
}

impl<'a, 'b> IntoIterator for &'a Image<'b> {
    type Item = ((usize, usize), Pixel);
    type IntoIter = PixelIterator<'a, 'b>;

    fn into_iter(self) -> Self::IntoIter {
        PixelIterator {
            position: (0,0),
            image: self
        }
    }
}

pub struct PixelIterator<'a, 'b> {
    position: (usize, usize),
    image: &'a Image<'b>
}

impl<'a, 'b> Iterator for PixelIterator<'a, 'b> {
    type Item = ((usize, usize), Pixel);

    fn next(&mut self) -> Option<Self::Item> {
        self.position.0 += 1;
        if self.position.0 >= self.image.width {
            self.position.1 += 1;
            self.position.0 = 0;
        }

        if self.position.1 >= self.image.height { return None; }
        Some((self.position, self.image.get_pixel(self.position.0, self.position.1)?))
    }
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
