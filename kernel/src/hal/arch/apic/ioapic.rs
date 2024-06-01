#[allow(unused_imports)] use crate::prelude::*;
use core::fmt::{Debug, Formatter};
use core::mem;
use acpi::{AcpiHandler, PhysicalMapping};
use ranged_btree::RangedBTreeMap;
use bit_field::BitField;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use kernel::hal::acpi::PagingReason;
use macros::Fields;
use crate::mmio::MmioCell;
use crate::projection::Project;

#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
struct VersionRegister(u32);

impl VersionRegister {
	pub fn version(self) -> u32 { self.0.get_bits(0..8) }
	pub fn max_redirection_entry(self) -> u32 { self.0.get_bits(16..24) }
}

#[derive(TryFromPrimitive, IntoPrimitive, Debug)]
#[repr(u32)]
pub enum DeliveryMode {
	Fixed = 0,
	LowestPriority = 1,
	Smi = 2,
	Reserved3 = 3,
	Nmi = 4,
	Init = 5,
	Reserved6 = 6,
	ExtInt = 7
}

#[derive(TryFromPrimitive, IntoPrimitive, Debug)]
#[repr(u32)]
pub enum DestinationMode {
	Physical = 0,
	Logical = 1,
}

pub struct Ioapics<H: AcpiHandler> {
	ioapics: RangedBTreeMap<usize, Ioapic<H>>, // would be nice if this could be made intrusive to not duplicate entry count between key and value
	legacy_map: LegacyMap,
}

impl<H: AcpiHandler> Ioapics<H> {
	pub const fn new() -> Self {
		Self {
			ioapics: RangedBTreeMap::new(),
			legacy_map: LegacyMap::pc_default(),
		}
	}

	pub fn push(&mut self, gsi: usize, ioapic: Ioapic<H>) {
		let range = gsi..(gsi + ioapic.size());
		self.ioapics.insert(range, ioapic).unwrap()
	}

	pub fn legacy_map(&mut self) -> &mut LegacyMap {
		&mut self.legacy_map
	}

	pub fn redirection_entry(&mut self, vector: u32) -> Option<RedirectionEntry<'_, H>> {
		let ioapic = self.ioapics.get_entry_at_point_mut(vector as usize)?;
		ioapic.redirection_entry(vector)
	}
}

impl<H: AcpiHandler> Debug for Ioapics<H> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mut d = f.debug_map();
		for (k, v) in &self.ioapics {
			d.entry(
				&format_args!("GSI {} -> {}", k.start, k.end),
				&format_args!("{:p}", v.mapping.virtual_start())
			);
		}
		d.finish()
	}
}

pub struct Ioapic<H: AcpiHandler> {
	mapping: PhysicalMapping<H, Registers>,
	cell: MmioCell<Registers>,
	num_entries: u32,
}

impl PagingReason for Registers {
	fn reason() -> u16 {
		crate::paging_codes::IOAPIC_REGISTERS
	}
}

impl<H: AcpiHandler> Ioapic<H> {
	pub(super) unsafe fn new(ioapic_base: usize, handler: H) -> Self {
		let mapping = unsafe { handler.map_physical_region::<Registers>(ioapic_base, mem::size_of::<Registers>()) };

		let mut this = Self {
			cell: unsafe { MmioCell::new(mapping.virtual_start().as_ptr()) },
			mapping,
			num_entries: 0,
		};

		this.num_entries = this.version_register().max_redirection_entry() + 1;

		this
	}

	pub fn version_register(&mut self) -> VersionRegister {
		self.cell.project::<Registers::select>().write(0x1);
		VersionRegister(self.cell.project::<Registers::data>().read())
	}

	pub fn size(&self) -> usize {
		self.num_entries.try_into().unwrap()
	}

	pub fn redirection_entry(&mut self, num: u32) -> Option<RedirectionEntry<'_, H>> {
		if num > self.num_entries { return None; }

		Some(RedirectionEntry {
			ioapic: self,
			num,
		})
	}
}

#[derive(Fields)]
#[repr(C)]
struct Registers {
	select: u32,
	_pad: [u32; 3],
	data: u32,
}

pub struct RedirectionEntry<'ioapic, H: AcpiHandler> {
	ioapic: &'ioapic mut Ioapic<H>,
	num: u32,
}

pub struct EntryData {
	pub vector: u8,
	pub delivery_mode: DeliveryMode,
	pub destination_mode: DestinationMode,
	pub polarity: ActiveLevel,
	pub trigger_mode: TriggerMode,
	pub mask: bool,
	pub destination: u8,
}

impl<H: AcpiHandler> RedirectionEntry<'_, H> {
	pub fn update(&mut self, f: impl FnOnce(&mut EntryData)) {
		let low_register = 0x10 + self.num*2;
		let high_register = 0x10 + self.num*2 + 1;

		self.ioapic.cell.project::<Registers::select>()
				.write(low_register);
		let low = self.ioapic.cell.project::<Registers::data>()
				.read();

		self.ioapic.cell.project::<Registers::select>()
				.write(high_register);
		let high = self.ioapic.cell.project::<Registers::data>()
				.read();

		let mut data = EntryData {
			vector: low.get_bits(0..8) as u8,
			delivery_mode: DeliveryMode::try_from(low.get_bits(8..=10)).unwrap(),
			destination_mode: DestinationMode::try_from(low.get_bits(11..=11)).unwrap(),
			polarity: ActiveLevel::try_from(low.get_bits(13..=13)).unwrap(),
			trigger_mode: TriggerMode::try_from(low.get_bits(15..=15)).unwrap(),
			mask: low.get_bit(16),
			destination: high.get_bits(24..=31) as u8,
		};

		f(&mut data);

		let new_low =
				u32::from(data.vector) |
				u32::from(data.delivery_mode) << 8 |
				u32::from(data.destination_mode) << 11 |
				u32::from(data.polarity) << 13 |
				u32::from(data.trigger_mode) << 15 |
				u32::from(data.mask) << 16;
		let new_high = u32::from(data.destination) << 24;

		self.ioapic.cell.project::<Registers::select>()
		    .write(low_register);
		self.ioapic.cell.project::<Registers::data>().write(new_low);

		self.ioapic.cell.project::<Registers::select>()
		    .write(high_register);
		self.ioapic.cell.project::<Registers::data>().write(new_high);
	}
}

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum TriggerMode {
	Level = 1,
	Edge = 0,
}

#[derive(Debug, Copy, Clone, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum ActiveLevel {
	High = 0,
	Low = 1,
}

#[derive(Debug, Copy, Clone)]
pub struct LegacyMap {
	pub pit: (u32, TriggerMode, ActiveLevel),
	pub ps2_keyboard: (u32, TriggerMode, ActiveLevel),
	pub com2: (u32, TriggerMode, ActiveLevel),
	pub com1: (u32, TriggerMode, ActiveLevel),
	pub lpt2: (u32, TriggerMode, ActiveLevel),
	pub floppy: (u32, TriggerMode, ActiveLevel),
	pub rtc: (u32, TriggerMode, ActiveLevel),
	pub ps2_mouse: (u32, TriggerMode, ActiveLevel),
	pub ata_primary: (u32, TriggerMode, ActiveLevel),
	pub ata_secondary: (u32, TriggerMode, ActiveLevel),
}

impl LegacyMap {
	pub const fn pc_default() -> Self {
		use TriggerMode::Edge;
		use ActiveLevel::High;
		
		Self {
			pit: (0, Edge, High),
			ps2_keyboard: (1, Edge, High),
			com2: (3, Edge, High),
			com1: (4, Edge, High),
			lpt2: (5, Edge, High),
			floppy: (6, Edge, High),
			rtc: (8, Edge, High),
			ps2_mouse: (12, Edge, High),
			ata_primary: (14, Edge, High),
			ata_secondary: (15, Edge, High),
		}
	}
}
