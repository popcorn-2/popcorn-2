#[allow(unused_imports)] use crate::prelude::*;
use core::fmt::{Debug, Formatter};
use bitflags::{bitflags, Flags};
use kernel_api::memory::{Frame, PhysicalAddress};
use crate::hal::paging::{Entry, Level};
use crate::hal::paging::levels::{Global, Upper, Middle, Lower};

impl Level for Global {
	#[allow(clippy::unusual_byte_groupings)] const MASK: usize = 0o777_000_000_000_0000;
	const SHIFT: usize = 12 + 9*3;
	type Entry = Amd64Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for Upper {
	#[allow(clippy::unusual_byte_groupings)] const MASK: usize = 0o777_000_000__0000;
	const SHIFT: usize = 12 + 9*2;
	type Entry = Amd64Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for Middle {
	#[allow(clippy::unusual_byte_groupings)] const MASK: usize = 0o777_000_0000;
	const SHIFT: usize = 12 + 9;
	type Entry = Amd64Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for Lower {
	#[allow(clippy::unusual_byte_groupings)] const MASK: usize = 0o777_0000;
	const SHIFT: usize = 12;
	type Entry = Amd64Entry;
	const ENTRY_COUNT: usize = 512;
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(transparent)]
pub struct Amd64Entry(pub u64);

bitflags! {
		impl Amd64Entry: u64 {
			const PRESENT = 1<<0;
			const WRITABLE = 1<<1;
			const ADDRESS = 0x000f_ffff_ffff_f000;
			const AVL_LOW = 0b111 << 9;
			const AVL_HIGH = 0x7ff0_0000_0000_0000;
			const PWT = 1<<3;
			const PCD = 1<<4;
		}
	}

impl Amd64Entry {
	// in future will take into account on-demand paging etc.
	pub(crate) fn is_used(self) -> bool { self.is_present() }
}

impl Entry for Amd64Entry {
	fn empty() -> Self {
		<Self as Flags>::empty()
	}

	fn is_present(self) -> bool { self.contains(Self::PRESENT) }

	fn pointed_frame(self) -> Option<Frame> {
		if !self.is_present() { return None; }

		let addr = self.0 & Self::ADDRESS.0;
		Some(Frame::new(PhysicalAddress::new(addr.try_into().unwrap())))
	}

	fn point_to_frame(&mut self, frame: Frame, reason: u16) -> Result<(), u16> {
		if self.is_present() {
			let low = (*self & Self::AVL_LOW).bits() >> 9;
			let high = (*self & Self::AVL_HIGH).bits() >> (52 - 3);
			return Err(u16::try_from(low | high).unwrap());
		}

		let reason = u64::from(reason);
		let low = (reason & 7) << 9;
		let high = (reason & 0x7ff8) << (52 - 3);
		let split_reason = low | high;
		let masked_addr = u64::try_from(frame.start().addr).unwrap() & Self::ADDRESS.0;
		// FIXME: HACK
		self.0 = masked_addr | Self::PRESENT.0 | Self::WRITABLE.0 | split_reason | Self::PWT.0 | Self::PCD.0;

		Ok(())
	}
}

impl Debug for Amd64Entry {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mut builder = f.debug_tuple("Amd64Entry");
		if let Some(f) = self.pointed_frame() {
			builder.field(&f);
		} else {
			builder.field(&"<unmapped>");
		}
		builder.finish()
	}
}
