use alloc::vec::Vec;
use crate::framebuffer::{Color, Framebuffer, OutOfBoundsError};
use crate::framebuffer::graphical::{Image, Rectangle};

pub struct Loading<'a> {
    logo: targa::Image<'a>,
    progress_bar_progress: u8
}

impl<'a> Loading<'a> {
    pub fn new(logo: targa::Image<'a>) -> Self {
        Self { logo, progress_bar_progress: 0 }
    }
}

impl<'a> Loading<'a> {
    pub fn draw_progress_bar(&self, fb: &mut Framebuffer, progress: u8) -> Result<(), OutOfBoundsError> {
        let bar_foreground = Rectangle::new(
            (img_origin.0, screen_center_y + 32),
            (self.logo.width / 100 * usize::from(self.progress_bar_progress), 4),
            Color::new(0xee, 0xee, 0xee, 0xff)
        );
        fb.draw(bar_foreground)
    }
}

impl<'a> super::Page for Loading<'a> {
    type Output = ();

    fn draw(&self, fb: &mut Framebuffer, (w, h): (usize, usize)) -> Result<Self::Output, OutOfBoundsError> {
        // background color
        let rect = Rectangle::new((0,0), (w, h), Color::new(0x33, 0x33, 0x33, 0xff));
        fb.draw(rect)?;

        // logo
        let (screen_center_x, screen_center_y) = (w / 2, h / 2);
        let img_origin = (screen_center_x - (self.logo.width / 2), screen_center_y - self.logo.height);
        let img = Image::new(img_origin, &self.logo);
        fb.draw(img)?;

        let bar_background = Rectangle::new(
            (img_origin.0, screen_center_y + 32),
            (self.logo.width, 4),
            Color::new(0x44, 0x44, 0x44, 0xff)
        );
        fb.draw(bar_background)?;

        let bar_foreground = Rectangle::new(
            (img_origin.0, screen_center_y + 32),
            (self.logo.width / 100 * usize::from(self.progress_bar_progress), 4),
            Color::new(0xee, 0xee, 0xee, 0xff)
        );
        fb.draw(bar_foreground)?;

        Ok(())
    }
}
