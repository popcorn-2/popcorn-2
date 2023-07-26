#![no_std]
#![feature(associated_type_defaults)]
#![feature(never_type)]

extern crate alloc;

pub mod image;
pub mod label;
pub mod window;
pub mod rect;
pub mod pixel;

pub trait Drawable {
    fn draw(&self) -> Result<(), Error>;
}

pub enum Error {}
