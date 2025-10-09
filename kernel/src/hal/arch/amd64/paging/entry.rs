use core::fmt::{Debug, Formatter};
use bitflags::bitflags;
use kernel_api::mapping::Ty;
use kernel_api::memory::RawFrame;
use crate::hal::paging2::Flags;
use crate::hal::paging::Entry;

#[derive(Copy, Clone, Eq, PartialEq, Default)]
#[repr(transparent)]
pub struct Amd64Entry(pub usize);

bitflags! {
	impl Amd64Entry: usize {
		const PRESENT = 1<<0;
		const WRITABLE = 1<<1;
		const ADDRESS = 0x000f_ffff_ffff_f000;
		const AVL_LOW = 0b111 << 9;
		const AVL_HIGH = 0x7ff0_0000_0000_0000;
		const PWT = 1<<3;
		const PCD = 1<<4;
		const USER = 1<<2;
		const NX = 1<<63;

		// prevent operators from truncating value
		const _ = !0;
	}
}

impl Amd64Entry {
	// in future will take into account on-demand paging etc.
	pub(crate) fn is_used(self) -> bool { self.is_present() }

	fn from_flags(flags: Flags) -> Self {
		let mut base = Self::PRESENT;
		if flags.contains(Flags::WRITE) { base |= Self::WRITABLE; }
		if !flags.contains(Flags::EXEC) { base |= Self::NX; }
		if flags.contains(Flags::USER) { base |= Self::USER; }
		if flags.contains(Flags::UNCACHED) { base |= Self::PCD; }
		if flags.contains(Flags::WRITE_COMBINE) { base |= Self::PWT; }
		base
	}
}

impl Entry for Amd64Entry {
	fn is_present(self) -> bool { self.contains(Self::PRESENT) }

	fn pointed_frame(self, debug: bool) -> Option<RawFrame> {
		if debug { debug!("check entry with val {:?} ({:#x})", self, self.0); }
		if !self.is_present() { return None; }

		let addr = self.0 & Self::ADDRESS.0;
		Some(RawFrame::new(addr))
	}

	fn point_to_frame(&mut self, frame: RawFrame, ty: Ty, flags: Flags) -> Result<(), Ty> {
		if self.is_present() {
			let low = (*self & Self::AVL_LOW).bits() >> 9;
			let high = (*self & Self::AVL_HIGH).bits() >> (52 - 3);
			return Err(u8::try_from(low | high).map(Ty).unwrap_or(Ty(u8::MAX)));
		}

		let reason = usize::from(ty.0);
		let low = (reason & 7) << 9;
		let high = (reason & 0x3ff8) << (52 - 3);
		let split_reason = low | high;
		let masked_addr = frame.addr & Self::ADDRESS.0;

		self.0 = masked_addr | split_reason | Self::from_flags(flags).0;

		Ok(())
	}
}

impl Debug for Amd64Entry {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "Amd64Entry(")?;
		if let Some(frame) = self.pointed_frame(false) {
			Debug::fmt(&frame, &mut *f)?;
		} else {
			write!(f, "--")?
		}
		write!(f, ", ")?;
		if self.contains(Amd64Entry::USER) { write!(f, "U")?; } else { write!(f, "S")?; }
		if self.contains(Amd64Entry::WRITABLE) { write!(f, "W")?; } else { write!(f, "R")?; }
		if self.contains(Amd64Entry::NX) { write!(f, "X")?; } else { write!(f, "E")?; }
		write!(f, ")")
	}
}
