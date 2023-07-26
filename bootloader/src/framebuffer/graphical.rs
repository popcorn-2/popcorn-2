use crate::framebuffer::{Color, Drawable, Framebuffer, OutOfBoundsError, Pixel};

#[derive(Debug, Copy, Clone)]
pub struct Rectangle {
    origin: (usize, usize),
    size: (usize, usize),
    color: Color
}

impl Rectangle {
    pub fn new(origin: (usize, usize), size: (usize, usize), color: Color) -> Rectangle {
        Self {
            origin, size, color
        }
    }
}

impl Drawable for Rectangle {
    type Output = ();

    fn draw(&self, framebuffer: &mut Framebuffer) -> Result<(), OutOfBoundsError> {
        let (x1, y1) = self.origin;
        let (x2, y2) = (x1 + self.size.0, y1 + self.size.1);
        for y in y1..y2 {
            for x in x1..x2 {
                let px = Pixel((x, y), self.color);
                framebuffer.draw(px)?;
            }
        }
        Ok(())
    }
}

impl From<((usize, usize), targa::Pixel)> for Pixel {
    fn from(value: ((usize, usize), targa::Pixel)) -> Self {
        Pixel(value.0, Color::new(value.1.r, value.1.g, value.1.b, value.1.a))
    }
}

pub struct Image<'a, T> {
    origin: (usize, usize),
    image: &'a T
}

impl<'a,T, P> Image<'a, T> where &'a T: IntoIterator<Item = P>, Pixel: From<P> {
    pub fn new(origin: (usize, usize), image: &'a T) -> Self {
        Self {
            origin,
            image
        }
    }
}

impl<'a, T, P> Drawable for Image<'a, T> where &'a T: IntoIterator<Item = P>, Pixel: From<P> {
    type Output = ();

    fn draw(&self, framebuffer: &mut Framebuffer) -> Result<Self::Output, OutOfBoundsError> {
        self.image.into_iter()
            .try_for_each(|px| {
                let mut px = Pixel::from(px);
                px.0.0 += self.origin.0;
                px.0.1 += self.origin.1;
                framebuffer.draw_pixel(px)
            })
    }
}
