use core::range::RangeInclusive;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PortSlice(RangeInclusive<u16>);

impl PortSlice {
	pub const ALL: Self = Self(RangeInclusive { start: 0, last: u16::MAX });

	pub const fn new(range: RangeInclusive<u16>) -> Self {
		Self(range)
	}

	pub const fn range(&self) -> &RangeInclusive<u16> {
		&self.0
	}

	pub const fn start(&self) -> u16 {
		self.0.start
	}

	pub const fn end(&self) -> u16 {
		self.0.last
	}

	pub fn contains(&self, port: &u16) -> bool {
		self.0.contains(port)
	}
}
