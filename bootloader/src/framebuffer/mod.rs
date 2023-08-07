/* pub mod graphical;
pub mod text;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt::Formatter;
use core::marker::PhantomData;
use core::{fmt, ptr};
use uefi::Error;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};

// TODO: Color format
pub struct Framebuffer<'a> {
	width: usize,
	height: usize,
	stride: usize,
	double_buffer: Box<[u8]>,
	actual_buffer: *mut u8,
	_phantom: PhantomData<&'a mut u8>
}

impl<'a> Framebuffer<'a> {
	pub fn new_from_gop(gop: &'a mut GraphicsOutput) -> Self {
		let optimal_resolutions = gop.modes()
				.filter(|mode|
						mode.info().resolution() == (1920, 1080) ||
						mode.info().resolution() == (1280, 720) ||
						mode.info().resolution() == (640, 480)
				);

		let optimal_resolution = optimal_resolutions.reduce(|acc, mode| {
			if mode.info().resolution().0 > acc.info().resolution().0 { mode }
			else { acc }
		});

		let actual_mode =
			if let Some(resolution) = optimal_resolution &&
			   gop.set_mode(&resolution).is_ok()
			{
				*resolution.info()
			} else {
				gop.current_mode_info()
			};

		let stride = actual_mode.stride();
		let (width, height) = actual_mode.resolution();

		let mut double_buffer_backing = Vec::with_capacity(stride * height * 4);
		// SAFETY: Is this unsafe? The memory exists, however its uninitialised, but then any bit pattern is valid for an int...
		unsafe { double_buffer_backing.set_len(stride * height * 4); }

		Self {
			width,
			height,
			stride,
			double_buffer: double_buffer_backing.into_boxed_slice(),
			actual_buffer: gop.frame_buffer().as_mut_ptr(),
			_phantom: PhantomData
		}
	}

	pub fn flush(&mut self) {
		// SAFETY:
		// double buffer is owned by self so must be valid
		// actual buffer is valid for lifetime of GOP handle which is same as lifetime of self
		unsafe {
			ptr::copy_nonoverlapping(self.double_buffer.as_ptr(), self.actual_buffer, self.stride * self.height * 4);
		}
	}

	pub fn get_buffer(&self) -> *const u8 { self.double_buffer.as_ptr() }
	pub fn get_buffer_mut(&mut self) -> *mut u8 { self.double_buffer.as_mut_ptr() }
	pub fn get_resolution(&self) -> (usize, usize) { (self.width, self.height) }
	pub fn get_stride(&self) -> usize { self.stride }

	pub fn draw<D: Drawable>(&mut self, object: D) -> Result<D::Output, OutOfBoundsError> {
		object.draw(self)
	}

	pub fn draw_pixel(&mut self, px: Pixel) -> Result<(), OutOfBoundsError> {
		let Pixel((x, y), color) = px;
		if x > self.width || y > self.height { return Err(OutOfBoundsError()); }

		if color.a == 0 { return Ok(()); }
		unsafe {
			// SAFETY: Checked bounds ourselves - writing directly should be faster than bounds checking on the slice in the box
			let ptr = self.get_buffer_mut() as *mut u32;
			let ptr = ptr.add(x + y * self.stride);

			if color.a == 255 { ptr.write(color.to_bgr()); }
			else {
				let dest_a = u16::from(!color.a);
				let dest_color = Color::from_bgr(ptr.read());
				let src_r = u16::from(color.r);
				let src_g = u16::from(color.g);
				let src_b = u16::from(color.b);
				let src_a = u16::from(color.a);
				let dest_r = u16::from(dest_color.r);
				let dest_g = u16::from(dest_color.g);
				let dest_b = u16::from(dest_color.b);
				let overall_r = (src_r * src_a + dest_r * dest_a + 0xFF) >> 8;
				let overall_g = (src_g * src_a + dest_g * dest_a + 0xFF) >> 8;
				let overall_b = (src_b * src_a + dest_b * dest_a + 0xFF) >> 8;
				let true_color = Color::new(overall_r as u8, overall_b as u8, overall_g as u8, 255);
				ptr.write(true_color.to_bgr());
			};
		}
		Ok(())
	}
}

impl<'a> fmt::Debug for Framebuffer<'a> {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		f.debug_struct("Framebuffer")
				.field("width", &self.width)
				.field("height", &self.height)
				.field("stride", &self.stride)
				.field("double_buffer", &self.double_buffer.len())
				.field("actual_buffer", &self.actual_buffer)
				.finish()
	}
}

#[derive(Debug, Copy, Clone)]
pub struct Pixel(pub (usize, usize), pub Color);

impl Drawable for Pixel {
	type Output = ();

	fn draw(&self, framebuffer: &mut Framebuffer) -> Result<Self::Output, OutOfBoundsError> {
		framebuffer.draw_pixel(*self)
	}
}

#[derive(Debug, Copy, Clone)]
pub struct Color {
	pub r: u8,
	pub g: u8,
	pub b: u8,
	pub a: u8
}

impl Color {
	pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
		Self { r, g, b, a }
	}

	pub(super) fn to_bgr(&self) -> u32 {
		u32::from(self.r) << 16 | u32::from(self.g) << 8 | u32::from(self.b)
	}

	pub(super) fn from_bgr(value: u32) -> Self {
		Self {
			r: ((value & 0xff0000) >> 16) as u8,
			g: ((value & 0x__ff00) >> 8) as u8,
			b:  (value & 0x____ff) as u8,
			a: 255
		}
	}
}

#[derive(Debug, Copy, Clone)]
pub struct OutOfBoundsError();

pub trait Drawable {
	type Output;
	fn draw(&self, framebuffer: &mut Framebuffer) -> Result<Self::Output, OutOfBoundsError>;
}

use ui::window::Backend;

pub struct BitBltBackend<'a> {
	gop: &'a mut GraphicsOutput
}

impl<'a> BitBltBackend<'a> {
	pub fn new(gop: &'a mut GraphicsOutput) -> Self {
		Self {
			gop
		}
	}
}

impl<'a> Backend for BitBltBackend<'a> {
	type Error = uefi::Error;

	fn flush(&mut self, buffer: &[ui::pixel::Color], width: usize, height: usize) -> Result<(), Self::Error> {
		let buffer = unsafe { &*(buffer as *const [ui::pixel::Color] as *const [BltPixel]) };
		let blt_op = BltOp::BufferToVideo {
			buffer,
			src: BltRegion::Full,
			dest: (0, 0),
			dims: (width, height),
		};

		self.gop.blt(blt_op)
	}
}
 */

use alloc::boxed::Box;
use alloc::vec;
use core::fmt;
use core::ops::{Deref, Index, IndexMut};
use core::ptr::slice_from_raw_parts;
use derive_more::Constructor;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};
use uefi::proto::unsafe_protocol;
use psf::PsfFont;
use crate::logging::FormatWrite;

#[derive(Constructor)]
pub struct FontFamily<'a> {
	regular: &'a dyn PsfFont,
	bold: Option<&'a dyn PsfFont>,
	italic: Option<&'a dyn PsfFont>,
	bold_italic: Option<&'a dyn PsfFont>
}

impl<'a> FontFamily<'a> {
	pub fn get_available_style(&self, style: FontStyle) -> FontStyle {
		if self.font_exists_for_style(style) { style }
		else {
			*style.fallbacks().iter()
			      .rev()
			      .reduce(|current, fallback| {
				      if self.font_exists_for_style(*fallback) { fallback }
				      else { current }
			      }).unwrap()
		}
	}

	fn font_exists_for_style(&self, style: FontStyle) -> bool {
		match style {
			FontStyle::Regular => true,
			FontStyle::Bold => self.bold.is_some(),
			FontStyle::Italic => self.italic.is_some(),
			FontStyle::BoldItalic => self.bold_italic.is_some()
		}
	}

	fn get_font_for_style(&self, style: FontStyle) -> &'a dyn PsfFont {
		match self.get_available_style(style) {
			FontStyle::Regular => self.regular,
			FontStyle::Bold => self.bold.unwrap(),
			FontStyle::Italic => self.italic.unwrap(),
			FontStyle::BoldItalic => self.bold_italic.unwrap()
		}
	}
}

#[derive(Copy, Clone, Debug)]
pub enum FontStyle {
	Regular,
	Bold,
	Italic,
	BoldItalic
}

impl FontStyle {
	pub fn fallbacks(self) -> &'static [Self] {
		match self {
			Self::Regular | Self::Bold | Self::Italic => &[Self::Regular],
			Self::BoldItalic => &[Self::Bold, Self::Italic, Self::Regular]
		}
	}
}

pub struct PixelBuffer(pub Box<[BltPixel]>, pub usize);

impl PixelBuffer {
	fn pixel(&self, x: usize, y: usize) -> &BltPixel { &self.0[x + (y * self.1)] }
	fn pixel_mut(&mut self, x: usize, y: usize) -> &mut BltPixel {
		&mut self.0[x + (y * self.1)]
	}
}

impl Index<(usize, usize)> for PixelBuffer {
	type Output = BltPixel;

	fn index(&self, index: (usize, usize)) -> &Self::Output {
		self.pixel(index.0, index.1)
	}
}

impl IndexMut<(usize, usize)> for PixelBuffer {
	fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
		self.pixel_mut(index.0, index.1)
	}
}

pub struct Tui<'a, 'b> {
	width: usize,
	height: usize,
	double_buffer: PixelBuffer,
	gop: &'a mut GraphicsOutput,
	font: FontFamily<'b>,
	location: (usize, usize),
	color: BltPixel,
	current_style: FontStyle
}

impl<'a, 'b> Tui<'a, 'b> {
	pub fn new(gop: &'a mut GraphicsOutput, native_resolution: (usize, usize), font: FontFamily<'b>) -> Self {
		let native_resolution = gop.modes().find(|mode| mode.info().resolution() == native_resolution);
		let optimal_resolutions = gop.modes()
		                             .filter(|mode|
				                             mode.info().resolution().1 == 1080 ||
						                             mode.info().resolution().1 == 720 ||
						                             mode.info().resolution().1 == 480
		                             );

		let mut optimal_resolution = optimal_resolutions.max_by_key(|mode| mode.info().resolution().0);

		if optimal_resolution.is_none() {
			let fallback_optimal_resolutions = gop.modes()
			                             .filter(|mode|
					                             mode.info().resolution().0 == 1920 ||
							                             mode.info().resolution().0 == 1280 ||
							                             mode.info().resolution().0 == 640
			                             );
			optimal_resolution = fallback_optimal_resolutions.max_by_key(|mode| mode.info().resolution().0);
		}

		let actual_mode =
				if let Some(resolution) = native_resolution.or(optimal_resolution) &&
						gop.set_mode(&resolution).is_ok()
				{
					*resolution.info()
				} else {
					gop.current_mode_info()
				};

		let (width, height) = actual_mode.resolution();

		let double_buffer_backing = vec![BltPixel::new(0, 0, 0); width * height];

		Self {
			width,
			height,
			double_buffer: PixelBuffer(double_buffer_backing.into_boxed_slice(), width),
			gop,
			font,
			location: (0, 0),
			color: BltPixel::new(0xee, 0xee, 0xee),
			current_style: FontStyle::Regular
		}
	}

	fn flush(&mut self) -> uefi::Result {
		let buffer = &*self.double_buffer.0;
		let blt_op = BltOp::BufferToVideo {
			buffer,
			src: BltRegion::Full,
			dest: (0, 0),
			dims: (self.width, self.height),
		};

		self.gop.blt(blt_op)
	}

	pub fn set_font_style(&mut self, style: FontStyle) {
		self.current_style = style;
	}

	pub fn set_font_color(&mut self, r: u8, g: u8, b: u8) {
		self.color = BltPixel::new(r, g, b);
	}

	fn shift_up(&mut self) {
		let font = self.font.get_font_for_style(self.current_style);
		let char_height = font.char_height();

		self.double_buffer.0.rotate_right(self.width * (self.height - char_height - 1));
		for i in &mut self.double_buffer.0[self.width * (self.height - char_height - 1)..self.width * self.height] {
			*i = BltPixel::new(0,0,0);
		}
	}

	fn newline(&mut self) {
		let font = self.font.get_font_for_style(self.current_style);
		let char_height = font.char_height();

		let new_y = self.location.1 + char_height + 1;
		if new_y + char_height >= self.height { // If any part of the next line is offscreen
			self.shift_up();
		} else {
			self.location.1 += char_height + 1;
		}
		self.location.0 = 0;
	}

	fn advance(&mut self, n: isize) {
		enum Direction { Left, Right }

		let font = self.font.get_font_for_style(self.current_style);
		let char_width = font.char_width();

		let direction = if n >= 0 { Direction::Right } else { Direction::Left };
		let advance_pixels = n.unsigned_abs() * (char_width + 1);

		match direction {
			Direction::Left => self.location.0 = self.location.0.saturating_sub(advance_pixels),
			Direction::Right => {
				self.location.0 += advance_pixels;
				if (self.location.0 + char_width) > self.width {
					self.newline();
				}
			}
		}
	}
}

impl<'a, 'b> fmt::Write for Tui<'a, 'b> {
	fn write_str(&mut self, s: &str) -> fmt::Result {
		let font = self.font.get_font_for_style(self.current_style);
		let (char_width, char_height) = (font.char_width(), font.char_height());

		for c in s.chars() {
			match c {
				'\n' => {
					self.newline();
					self.flush().map_err(|_| fmt::Error)?;
				},
				'\t' => self.advance(4),
				c => {
					let c = font.locate_char(c).map_err(|_| fmt::Error)?;

					for y in 0..char_height {
						for x in 0..char_width {
							let draw_loc = (self.location.0 + x, self.location.1 + y);
							if c.is_set(x, y) { self.double_buffer[draw_loc] = self.color; }
						}
					}

					self.advance(1);
				}
			}
		}

		Ok(())
	}
}

impl<'a, 'b> FormatWrite for Tui<'a, 'b> {
	fn set_color(&mut self, color: (u8, u8, u8)) {
		self.color = BltPixel::new(color.0, color.1, color.2);
	}

	fn set_font_style(&mut self, style: FontStyle) {
		self.current_style = style;
	}
}

#[repr(C)]
#[unsafe_protocol("bd8c1056-9f36-44ec-92a8-a6337f817986")]
pub struct ActiveEdid {
	edid_size: u32,
	edid_data: *const u8
}

impl Deref for ActiveEdid {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		unsafe { &*slice_from_raw_parts(self.edid_data, self.edid_size.try_into().unwrap()) }
	}
}
