use core::fmt::{Debug, Formatter};
use bitflags::{bitflags, Flags};
use kernel_api::memory::{Frame, PhysicalAddress};
use crate::paging::{Entry, Level};
use crate::paging::levels::{Global, Upper, Middle, Lower};

impl Level for Global {
	const MASK: usize = 0o777_000_000_000_0000;
	const SHIFT: usize = 12 + 9*3;
	type Entry = Amd64Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for Upper {
	const MASK: usize = 0o777_000_000__0000;
	const SHIFT: usize = 12 + 9*2;
	type Entry = Amd64Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for Middle {
	const MASK: usize = 0o777_000_0000;
	const SHIFT: usize = 12 + 9*1;
	type Entry = Amd64Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for Lower {
	const MASK: usize = 0o777_0000;
	const SHIFT: usize = 12 + 9*0;
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
			const ADDRESS = 0x0fff_ffff_ffff_f000;
		}
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

	fn point_to_frame(&mut self, frame: Frame) -> Result<(), ()> {
		if self.is_present() { return Err(()); }

		let empty_entry = self.0 & !Self::ADDRESS.0;
		let masked_addr = u64::try_from(frame.start().addr).unwrap() & Self::ADDRESS.0;
		self.0 = empty_entry | masked_addr | Self::PRESENT.0 | Self::WRITABLE.0;

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
