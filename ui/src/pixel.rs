#[repr(C)]
pub struct Color {
	pub blue: u8,
	pub green: u8,
	pub red: u8,
	_reserved: u8,
}

pub struct Color2 {
	pub blue: u8,
	pub green: u8,
	pub red: u8,
	pub alpha: u8,
}

pub struct Coordinate(pub usize, pub usize);

pub struct Pixel(pub Color2, pub Coordinate);
