use bitflags::{bitflags, Flags};
use kernel_api::memory::{Frame, PhysicalAddress};
use crate::paging::{Entry, Level};
use crate::paging::levels::{L4, L3, L2, L1};

impl Level for L4 {
	const MASK: usize = 0o777_000_000_000_0000;
	const SHIFT: usize = 12 + 9*3;
	type Entry = Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for L3 {
	const MASK: usize = 0o777_000_000__0000;
	const SHIFT: usize = 12 + 9*2;
	type Entry = Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for L2 {
	const MASK: usize = 0o777_000_0000;
	const SHIFT: usize = 12 + 9*1;
	type Entry = Entry;
	const ENTRY_COUNT: usize = 512;
}

impl Level for L1 {
	const MASK: usize = 0o777_0000;
	const SHIFT: usize = 12 + 9*0;
	type Entry = Entry;
	const ENTRY_COUNT: usize = 512;
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(transparent)]
pub struct Entry(u64);

bitflags! {
		impl Entry: u64 {
			const PRESENT = 1<<0;
			const ADDRESS = 0x0fff_ffff_ffff_f000;
		}
	}

impl Entry for Entry {
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
		self.0 = empty_entry | masked_addr | Self::PRESENT.0;

		Ok(())
	}
}
