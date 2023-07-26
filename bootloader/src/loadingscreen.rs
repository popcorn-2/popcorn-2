use core::fmt;
use ui::image::Image;
use ui::pixel::Color2;
use ui::rect::Rectangle;

pub struct LoadingScreen<'a, T> {
	pub background: Rectangle,
	pub logo: Image<'a, T>,
	pub progress_bar: ProgressBar,
	pub text: TextArea
}

impl<'a, T> LoadingScreen<'a, T> {
	pub fn new(dimensions: (usize, usize), bg_color: Color2, logo: Image<'a, T>) -> Self {
		Self {
			background: Rectangle::new((0,0), dimensions, bg_color),
			logo,
			progress_bar: ProgressBar::new(todo!(), todo!()),
			text: TextArea
		}
	}

	pub fn new_with_progress(dimensions: (usize, usize), bg_color: Color2, logo: Image<'a, T>, progress: u8) -> Self {
		let screen = Self::new(dimensions, bg_color, logo);
		screen.progress_bar.set_progress(progress);
		screen
	}
}

pub struct ProgressBar {
	background: Rectangle,
	foreground: Rectangle
}

impl ProgressBar {
	pub fn new(origin: (usize, usize), size: (usize, usize)) -> Self {
		todo!()
	}

	pub fn set_progress(progress: u8) {
		todo!("Calculate new foreground rectangle and redraw")
	}
}

pub struct TextArea;

impl fmt::Write for TextArea {
	fn write_str(&mut self, s: &str) -> fmt::Result {
		todo!()
	}
}
