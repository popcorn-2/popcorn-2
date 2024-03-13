use core::mem;
use core::ptr::addr_of;
use acpi::AcpiHandler;
use acpi::bgrt::Bgrt;
use crate::hal::acpi::{Handler, XPhysicalMapping, AcpiHandlerExt};

#[repr(C, packed)]
struct BmpHeader {
	magic: [u8; 2],
	size: u32,
	_res: [u8; 4],
	pixel_offset: u32,
	dib_header_size: u32,
	width: i32,
	height: i32,
	color_plane_count: u16,
	bpp: u16,
	compression: u32,
}

#[derive(Debug)]
pub struct Bmp<'a> {
	pub width: i32,
	pub height: i32,
	pub bpp: u16,
	mapping: XPhysicalMapping<Handler<'a>, [u8]>,
}

pub struct BmpIterator<'a> {
	pub bmp: Bmp<'a>,
	x: usize,
	y: usize,
}

impl Iterator for BmpIterator<'_> {
	type Item = u32;

	fn next(&mut self) -> Option<Self::Item> {
		if self.y >= self.bmp.height as usize { return None; }

		let bytes_per_pixel = self.bmp.bpp / 8;
		let stride = ((bytes_per_pixel as i32) * self.bmp.width + 3) / 4 * 4;
		let coord_y = (self.bmp.height as usize) - self.y - 1;
		let coord = self.x*(bytes_per_pixel as usize) + coord_y*(stride as usize);

		let ret = match bytes_per_pixel {
			3 => {
				let b = self.bmp.mapping[coord];
				let g = self.bmp.mapping[coord+1];
				let r = self.bmp.mapping[coord+2];
				((r as u32) << 16) | ((g as u32) << 8) | b as u32
			}
			4 => {
				let b = self.bmp.mapping[coord+1];
				let g = self.bmp.mapping[coord+2];
				let r = self.bmp.mapping[coord+3];
				((r as u32) << 16) | ((g as u32) << 8) | b as u32
			}
			_ => unimplemented!()
		};

		self.x += 1;
		if self.x >= self.bmp.width as usize {
			self.y += 1;
			self.x = 0;
		}

		Some(ret)
	}
}

impl<'a> IntoIterator for Bmp<'a> {
	type Item = u32;
	type IntoIter = BmpIterator<'a>;

	fn into_iter(self) -> Self::IntoIter {
		BmpIterator {
			bmp: self,
			x: 0,
			y: 0,
		}
	}
}

pub fn from_bgrt<'a>(bgrt: &Bgrt, handler: Handler<'a>) -> Option<Bmp<'a>> {
	let addr = bgrt.image_address;
	let header = unsafe {
		handler.map_physical_region::<BmpHeader>(addr as usize, mem::size_of::<BmpHeader>())
	};

	if header.dib_header_size < 40 { return None; }
	if header.bpp != 24 && header.bpp != 32 { return None; }
	if header.color_plane_count != 1 { return None; }
	if unsafe { addr_of!(header.compression).read_unaligned() } != 0 { return None; }

	let pixels_size = header.size - header.pixel_offset;
	let bmp_pixels = (addr + u64::from(header.pixel_offset)) as usize;

	let pixels = unsafe {
		handler.map_region::<[u8]>(bmp_pixels, pixels_size as usize, pixels_size as usize)
	};

	Some(Bmp {
		width: header.width,
		height: header.height,
		bpp: header.bpp,
		mapping: pixels
	})
}
