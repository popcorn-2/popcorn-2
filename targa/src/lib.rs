#![no_std]

mod pixel;
mod header;
mod errors;
pub use pixel::*;

use crate::errors::ParsingError;

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
