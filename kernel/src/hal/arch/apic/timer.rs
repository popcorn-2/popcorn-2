use bit_field::BitField;
use num_enum::IntoPrimitive;

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct Lvt(u32);

impl Lvt {
	pub fn with_mode(mut self, mode: TimerMode) -> Self {
		Self(*self.0.set_bits(17..=18, mode.into()))
	}

	pub fn with_mask(mut self, is_masked: bool) -> Self {
		Self(*self.0.set_bit(16, is_masked))
	}

	pub fn with_vector(mut self, vector: u8) -> Self {
		Self(*self.0.set_bits(0..8, vector.into()))
	}
}

#[derive(IntoPrimitive)]
#[repr(u32)]
pub enum TimerMode {
	OneShot = 0,
	Periodic = 1,
	Tsc = 2
}
