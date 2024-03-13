use core::fmt::{Debug, Formatter};
use core::mem;
use core::mem::offset_of;
use acpi::{AcpiHandler, PhysicalMapping};
use ranged_btree::RangedBTreeMap;
use bit_field::BitField;
use num_enum::{IntoPrimitive, TryFromPrimitive};
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

pub struct Ioapics<H: AcpiHandler> {
	ioapics: RangedBTreeMap<usize, Ioapic<H>>, // would be nice if this could be made intrusive to not duplicate entry count between key and value
}

impl<H: AcpiHandler> Ioapics<H> {
	pub fn new() -> Self {
		Self {
			ioapics: RangedBTreeMap::new(),
		}
	}

	pub fn push(&mut self, gsi: usize, ioapic: Ioapic<H>) {
		let range = gsi..(gsi + ioapic.size());
		self.ioapics.insert(range, ioapic).unwrap()
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

impl<H: AcpiHandler> Ioapic<H> {
	pub(super) unsafe fn new(ioapic_base: usize, handler: H) -> Self {
		let mapping = unsafe { handler.map_physical_region::<Registers>(ioapic_base, mem::size_of::<Registers>()) };

		let mut temp_self = Self {
			cell: unsafe { MmioCell::new(mapping.virtual_start().as_ptr()) },
			mapping,
			num_entries: 0,
		};

		temp_self.num_entries = temp_self.version_register().max_redirection_entry() + 1;

		temp_self
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

#[derive(Debug, Copy, Clone)]
pub enum TriggerMode {
	Level,
	Edge
}

#[derive(Debug, Copy, Clone)]
pub enum ActiveLevel {
	High,
	Low,
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
