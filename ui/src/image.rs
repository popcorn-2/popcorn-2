use crate::pixel::{Color, Coordinate};

pub struct Image<'a, T> {
	origin: Coordinate,
	image: &'a T
}

impl<'a,T, P> Image<'a, T> where &'a T: IntoIterator<Item = P>, Color: From<P> {
	pub fn new(origin: Coordinate, image: &'a T) -> Self {
		Self {
			origin,
			image
		}
	}
}

impl From<((usize, usize), targa::Pixel)> for Color {
	fn from(value: ((usize, usize), targa::Pixel)) -> Self {
		Color()
	}
}

impl<'a, T, P> Drawable for Image<'a, T> where &'a T: IntoIterator<Item = P>, Color: From<P> {
	type Output = ();

	fn draw(&self, framebuffer: &mut Framebuffer) -> Result<Self::Output, OutOfBoundsError> {
		self.image.into_iter()
		    .try_for_each(|px| {
			    let mut px = Color::from(px);
			    px.0.0 += self.origin.0;
			    px.0.1 += self.origin.1;
			    framebuffer.draw_pixel(px)
		    })
	}
}
