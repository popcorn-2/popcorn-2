use core::fmt::{Debug, Formatter};
use bit_field::BitField;
use macros::Fields;

#[repr(C)]
#[derive(Fields, Debug, Copy, Clone)]
pub struct Timer {
	capabilities: Capabilities,
	comparator: u64,
	fsb_route: u64,
	_res: u64,
}

#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct Capabilities(u64);

impl Capabilities {
	pub fn routing_capability(self) -> u64 {
		self.0.get_bits(32..64)
	}

	pub fn fsb_interrupts_supported(self) -> bool {
		self.0.get_bit(15)
	}

	pub fn fsb_interrupts_enabled(self) -> bool {
		self.0.get_bit(14)
	}

	pub fn set_fsb_interrupts_enabled(mut self, enabled: bool) -> Self {
		Self(*self.0.set_bit(14, enabled))
	}

	pub fn is_64_bit(self) -> bool {
		self.0.get_bit(5)
	}

	pub fn periodic_supported(self) -> bool {
		self.0.get_bit(4)
	}

	pub fn trigger_mode(self) -> TriggerMode {
		if self.0.get_bit(1) { TriggerMode::Level } else { TriggerMode::Edge }
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TriggerMode {
	Edge,
	Level,
}

impl Debug for Capabilities {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		struct MappingFormatHelper(u64);

		impl Debug for MappingFormatHelper {
			fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
				let mut d = f.debug_set();

				for bit_idx in 0..core::mem::size_of_val(&self.0) {
					let bit = self.0.get_bit(bit_idx);
					if bit { d.entry(&bit_idx); }
				}

				d.finish()
			}
		}

		f.debug_struct("Capabilities")
				.field("routing_capability", &MappingFormatHelper(self.routing_capability()))
				.field("fsb_interrupts_supported", &self.fsb_interrupts_supported())
				.field("fsb_interrupts_enabled", &self.fsb_interrupts_enabled())
				.field("64_bit", &self.is_64_bit())
				.field("periodic_supported", &self.periodic_supported())
				.field("trigger_mode", &self.trigger_mode())
				.finish_non_exhaustive()
	}
}
