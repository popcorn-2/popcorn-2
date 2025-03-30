#[allow(unused_imports)] use crate::prelude::*;
use core::fmt::{Debug, Formatter};
use bit_field::BitField;
use num_enum::{IntoPrimitive, TryFromPrimitive};

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
	
	pub fn mode(&self) -> TimerMode {
		TimerMode::try_from(self.0.get_bits(17..=18)).unwrap()
	}
}

impl Debug for Lvt {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mode = self.mode();
		let mask = self.0.get_bit(16);
		let vector = self.0.get_bits(0..8);
		f.debug_struct("Lvt")
				.field("mode", &mode)
				.field("masked", &mask)
				.field("vector", &vector)
				.finish()
	}
}

#[derive(IntoPrimitive, TryFromPrimitive, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum TimerMode {
	OneShot = 0,
	Periodic = 1,
	Tsc = 2,
	Reserved = 3,
}
