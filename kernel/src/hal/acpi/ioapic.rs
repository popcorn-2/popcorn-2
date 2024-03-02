use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt::{Debug, Formatter};
use acpi::{AcpiHandler, PhysicalMapping};
use bit_field::BitField;
use num_enum::{IntoPrimitive, TryFromPrimitive};

pub struct Ioapics {
	//ioapics: Vec<Ioapic>
}

pub struct Ioapic<H: AcpiHandler> {
	pub mapping: PhysicalMapping<H, u32>,
	pub select_register: RefCell<()>
}

impl<H: AcpiHandler> Ioapic<H> {
	fn write_at(&mut self, offset: u32, value: u32) {
		unsafe {
			let _guard = self.select_register.get_mut();
			self.mapping.virtual_start().as_ptr().write_volatile(offset);
			self.mapping.virtual_start().as_ptr().byte_add(0x10).write_volatile(value);
		}
	}

	fn read_at(&self, offset: u32) -> u32 {
		unsafe {
			let _guard = self.select_register.borrow_mut();
			self.mapping.virtual_start().as_ptr().write_volatile(offset);
			self.mapping.virtual_start().as_ptr().byte_add(0x10).read_volatile()
		}
	}

	pub fn version(&self) -> u8 {
		self.read_at(1).get_bits(0..8) as u8
	}

	pub fn irq_count(&self) -> u16 {
		(self.read_at(1).get_bits(16..24) + 1) as u16
	}

	pub fn redirection_entry(&self, number: u16) -> Option<RedirectionEntry<'_, H>> {
		if number >= self.irq_count() { return None; }

		Some(RedirectionEntry {
			ioapic: self,
			number: number.into()
		})
	}
}

impl<H: AcpiHandler> Debug for Ioapic<H> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		struct RedirectionEntries<'a, H: AcpiHandler>(&'a Ioapic<H>);
		impl<H: AcpiHandler> Debug for RedirectionEntries<'_, H> {
			fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
				let mut d = f.debug_list();
				for i in 0..self.0.irq_count() {
					d.entry(self.0.redirection_entry(i).as_ref().unwrap());
				}
				d.finish()
			}
		}

		f.debug_struct("Ioapic")
				.field("version", &self.version())
				.field("irq_count", &self.irq_count())
				.field("redirection_entries", &RedirectionEntries(self))
				.finish()
	}
}

struct RedirectionEntry<'ioapic, H: AcpiHandler> {
	ioapic: &'ioapic Ioapic<H>,
	number: u32
}

macro_rules! reg_subset {
    ($vis:vis $name: ident, $start: literal, $size: literal, $ret: ty) => {
	    reg_subset!($vis $name, $start, $size, $ret, |x: u64| x.try_into().unwrap());
    };

	($vis:vis $name: ident, $start: literal, $size: literal, $ret: ty, $conv: expr) => {
	    $vis fn $name(&self) -> $ret {
		    let conv = $conv;
		    conv(self.read().get_bits($start..($start + $size)))
	    }

	    /*paste::paste! {
			$vis fn [<set_ $name>] (&mut self, value: $ret) {
				let mut v = self.read();
			    v.set_bits($start..($start + $size), value.try_into().unwrap());
				self.write(v);
		    }
	    }*/
    };
}

impl<H: AcpiHandler> RedirectionEntry<'_, H> {
	fn read(&self) -> u64 {
		let base = 0x10 + self.number*2;
		let a = self.ioapic.read_at(base);
		let b = self.ioapic.read_at(base + 1);
		(b as u64) << 32 | (a as u64)
	}

	/*fn write(&mut self, val: u64) {
		let base = 0x10 + self.number*2;
		self.ioapic.write_at(base, val as u32);
		self.ioapic.write_at(base + 1, (val >> 32) as u32);
	}*/

	reg_subset!(pub vector, 0, 8, u8);
	reg_subset!(pub delivery_mode, 8, 3, DeliveryMode);
	reg_subset!(pub destination_mode, 11, 1, DestinationMode);
	reg_subset!(pub destination, 56, 8, u8);
	reg_subset!(pub mask, 16, 1, bool, |x| x != 0);
}

impl<H: AcpiHandler> Debug for RedirectionEntry<'_, H> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("RedirectionEntry")
				.field("vector", &self.vector())
				.field("delivery_mode", &self.delivery_mode())
				.field("destination_mode", &self.destination_mode())
				.field("destination", &self.destination())
				.field("mask", &self.mask())
				.finish()
	}
}

// FIXME: should be repr(u8) but needs to be u64 for macro stuff above to work
#[derive(TryFromPrimitive, IntoPrimitive, Debug)]
#[repr(u64)]
enum DeliveryMode {
	Fixed = 0,
	LowestPriority = 1,
	Smi = 2,
	Nmi = 4,
	Init = 5,
	ExtInt = 7
}

#[derive(TryFromPrimitive, IntoPrimitive, Debug)]
#[repr(u64)]
enum DestinationMode {
	Physical = 0,
	Logical = 1,
}
