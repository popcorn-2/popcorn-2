use crate::framebuffer::{Drawable, Framebuffer, OutOfBoundsError};

pub mod main;
pub mod loading;

pub use loading::Loading;

pub trait Page {
    type Output;

    fn draw(&self, framebuffer: &mut Framebuffer, resolution: (usize, usize)) -> Result<Self::Output, OutOfBoundsError>;
}

impl<T: Page> Drawable for T {
    type Output = T::Output;

    fn draw(&self, framebuffer: &mut Framebuffer) -> Result<Self::Output, OutOfBoundsError> {
        self.draw(framebuffer, framebuffer.get_resolution())
    }
}
