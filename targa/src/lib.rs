#![no_std]

mod pixel;
mod header;
pub mod errors;
pub use pixel::*;

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "alloc")]
use alloc::borrow;

use core::mem;
use crate::errors::ParsingError;

/// A clone-on-write Targa image
///
/// If the `alloc` feature is disabled, only immutable access to the image will be allowed
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
    /// Retreives a pixel from the image
    ///
    /// If the pixel is out of bounds of the image, returns [`Option::None`](None)
    ///
    /// # Todo
    ///
    /// Currently does not support Rgb555 format and will panic if attempted
    #[must_use]
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

/// An iterator over the pixels of an image
///
/// Returns an object of `((usize, usize), Pixel)` for each pixel, which gives the x and y coordinates as well as the pixel value
pub struct PixelIterator<'a, 'b> {
    position: (usize, usize),
    image: &'a Image<'b>
}

impl<'a, 'b> Iterator for PixelIterator<'a, 'b> {
    type Item = ((usize, usize), Pixel);

    fn next(&mut self) -> Option<Self::Item> {
        if self.position.1 >= self.image.height { return None; }

        let ret = Some((self.position, self.image.get_pixel(self.position.0, self.position.1)?));

        self.position.0 += 1;
        if self.position.0 >= self.image.width {
            self.position.1 += 1;
            self.position.0 = 0;
        }

        ret
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
            PixelFormat::Rgb555 | PixelFormat::Rgba5551 => 2,
            PixelFormat::Rgb888 => 3,
            PixelFormat::Rgba8888 => 4,
        }
    }
}

impl<'a> Image<'a> {
    /// Create a targa image object from a memory buffer
    ///
    /// # Errors
    ///
    /// Returns a [`ParsingError::NoHeader`] if the buffer is too short to contain a valid header.
    /// Returns a [`ParsingError::NotEnoughData`] if the buffer is too short to contain enough image data for the resolution specified in the header.
    /// Returns a [`ParsingError::Unsupported`] if the image uses an unsupported color encoding.
    pub fn try_new(data_raw: &'a [u8]) -> Result<Self, ParsingError> {
        if data_raw.len() < mem::size_of::<header::Header>() { return Err(ParsingError::NoHeader); }

        // SAFETY: All fields are integers and data is long enough
        // Alignment?
        let data = unsafe { &*(data_raw.as_ptr().cast::<header::Header>()) };
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

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;
    use alloc::vec;
    use crate::{Image, Pixel, PixelFormat, PixelOrderHorizontal, PixelOrderVertical};

    #[test]
    fn returns_none_for_out_of_bounds_pixel() {
        let test_image = Image {
            pixel_data: Cow::Borrowed(&[]),
            width: 0,
            height: 0,
            color_format: PixelFormat::Rgba8888,
            ordering: (PixelOrderVertical::TopToBottom, PixelOrderHorizontal::LeftToRight),
        };

        assert!(test_image.get_pixel(5, 7).is_none());
    }

    #[test]
    fn horizontal_pixel_ordering_correct() {
        // Targa is stored as BGRA
        const PIX_1: [u8; 4] = [0xaa, 0xbb, 0xcc, 0xff];
        const PIX_2: [u8; 4] = [0x11, 0x22, 0x33, 0xff];

        let mut test_image_data = vec![];
        test_image_data.extend(PIX_1);
        test_image_data.extend(PIX_2);

        let test_image_ltr = Image {
            pixel_data: Cow::Borrowed(&test_image_data),
            width: 2,
            height: 1,
            color_format: PixelFormat::Rgba8888,
            ordering: (PixelOrderVertical::TopToBottom, PixelOrderHorizontal::LeftToRight),
        };

        let test_image_rtl = Image {
            pixel_data: Cow::Borrowed(&test_image_data),
            width: 2,
            height: 1,
            color_format: PixelFormat::Rgba8888,
            ordering: (PixelOrderVertical::TopToBottom, PixelOrderHorizontal::RightToLeft),
        };

        let ltr_0_0 = test_image_ltr.get_pixel(0, 0).unwrap();
        assert_eq!(ltr_0_0, Pixel { r: PIX_1[2], g: PIX_1[1], b: PIX_1[0], a: PIX_1[3] });
        let ltr_1_0 = test_image_ltr.get_pixel(1, 0).unwrap();
        assert_eq!(ltr_1_0, Pixel { r: PIX_2[2], g: PIX_2[1], b: PIX_2[0], a: PIX_2[3] });

        let rtl_0_0 = test_image_rtl.get_pixel(0, 0).unwrap();
        assert_eq!(rtl_0_0, Pixel { r: PIX_2[2], g: PIX_2[1], b: PIX_2[0], a: PIX_2[3] });
        let rtl_1_0 = test_image_rtl.get_pixel(1, 0).unwrap();
        assert_eq!(rtl_1_0, Pixel { r: PIX_1[2], g: PIX_1[1], b: PIX_1[0], a: PIX_1[3] });
    }

    #[test]
    fn decodes_rgb888_correctly() {
        let test_image_data = vec![0xaa, 0xbb, 0xcc];

        let test_image = Image {
            pixel_data: Cow::Borrowed(&test_image_data),
            width: 1,
            height: 1,
            color_format: PixelFormat::Rgb888,
            ordering: (PixelOrderVertical::TopToBottom, PixelOrderHorizontal::LeftToRight),
        };

        let pix_0_0 = test_image.get_pixel(0, 0).unwrap();
        assert_eq!(pix_0_0, Pixel{ r: 0xcc, g: 0xbb, b: 0xaa, a: 255 });
    }

    #[test]
    fn pixel_iterator_yields_correct_data() {
        let test_image_data = vec![0xaa, 0xbb, 0xcc, 0x11, 0x22, 0x33, 0xdd, 0xee, 0xff, 0x44, 0x55, 0x66];

        let test_image = Image {
            pixel_data: Cow::Borrowed(&test_image_data),
            width: 2,
            height: 2,
            color_format: PixelFormat::Rgb888,
            ordering: (PixelOrderVertical::TopToBottom, PixelOrderHorizontal::LeftToRight),
        };

        let mut iter = test_image.into_iter();
        assert_eq!(iter.next(), Some((
            (0, 0), Pixel{ r: 0xcc, g: 0xbb, b: 0xaa, a: 255 }
        )));
        assert_eq!(iter.next(), Some((
            (1, 0), Pixel{ r: 0x33, g: 0x22, b: 0x11, a: 255 }
        )));
        assert_eq!(iter.next(), Some((
            (0, 1), Pixel{ r: 0xff, g: 0xee, b: 0xdd, a: 255 }
        )));
        assert_eq!(iter.next(), Some((
            (1, 1), Pixel{ r: 0x66, g: 0x55, b: 0x44, a: 255 }
        )));
        assert_eq!(iter.next(), None);
    }
}
