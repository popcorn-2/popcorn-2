use alloc::rc::{self, Rc};
use alloc::vec::Vec;
use crate::Drawable;
use crate::pixel::Color;

pub struct Window<B: Backend> {
    backend: B
}

impl<B: Backend> Window<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend
        }
    }
}

pub trait Backend {
    type Error = ();
    fn flush(&mut self, buffer: &[Color], width: usize, height: usize) -> Result<(), Self::Error>;
}
