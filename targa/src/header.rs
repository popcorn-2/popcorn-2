use crate::{ParsingError, PixelFormat, PixelOrderHorizontal, PixelOrderVertical};

#[repr(C, packed)]
pub struct Header {
    image_id_length: u8,
    color_map: u8,
    image_type: Type,
    color_map_spec: ColorMapSpec,
    x_origin: u16,
    y_origin: u16,
    width: u16,
    height: u16,
    color_depth: u8,
    image_descriptor: u8
}

impl Header {
    pub fn color_format(&self) -> Result<PixelFormat, ParsingError> {
        match self.color_depth {
            32 => Ok(PixelFormat::Rgba8888),
            24 => Ok(PixelFormat::Rgb888),
            _ => Err(ParsingError::Unsupported)
        }
    }

    pub fn width(&self) -> usize {
        self.width.into()
    }

    pub fn height(&self) -> usize {
        self.height.into()
    }

    pub fn pixel_ordering(&self) -> (PixelOrderVertical, PixelOrderHorizontal) {
        let ordering_horizontal = match self.image_descriptor & (1<<4) {
            0 => PixelOrderHorizontal::LeftToRight,
            _ => PixelOrderHorizontal::RightToLeft
        };
        let ordering_vertical = match self.image_descriptor & (1<<5) {
            0 => PixelOrderVertical::BottomToTop,
            _ => PixelOrderVertical::TopToBottom
        };
        (ordering_vertical, ordering_horizontal)
    }

    pub fn has_color_map(&self) -> bool {
        self.color_map != 0
    }

    pub fn image_format(&self) -> Type {
        self.image_type
    }
}

#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
#[repr(u8)]
pub enum Type {
    NoData = 0,
    UncompressedColorMapped = 1,
    UncompressedTrueColor = 2,
    UncompressedGreyscale = 3,
    RleColorMapped = 9,
    RleTrueColor = 10,
    RleGreyscale = 11,
}

#[repr(C, packed)]
struct ColorMapSpec {
    first_entry_index: u16,
    length: u16,
    entry_size: u8
}
