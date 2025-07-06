#[allow(unused_imports)] use crate::prelude::*;
use core::fmt::{Debug, Formatter};
use core::mem;
use core::num::NonZero;
use core::ops::Deref;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU32, Ordering};
use acpi::{AcpiHandler, PhysicalMapping};
use acpi::madt::IoApicEntry;
use ranged_btree::RangedBTreeMap;
use bit_field::BitField;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use kernel::hal::acpi::PagingReason;
use kernel_api::memory::mapping::{Config, Location, Mapping};
use kernel_api::memory::{Frame, PhysicalAddress};
use kernel_api::memory::r#virtual::Global;
use kernel_api::sync::{Spinlock, SpinlockGuard};
use macros::Fields;
use crate::hal::interrupts2electricboogaloo::Line;
use crate::mmio::MmioCell;
use crate::projection::Project;
use crate::volatile_ext::Volatile;

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

pub struct Ioapic {
	mmap: Mapping<'static>,
	offset: usize,
	entry_count: u32,
	gsi_base: Line,
}

impl Ioapic {
	pub(super) fn init(entry: &IoApicEntry) -> Ioapic {
		let IoApicEntry { io_apic_address, global_system_interrupt_base, .. } = *entry;
		let global_system_interrupt_base = global_system_interrupt_base.into()

		let (mmap, offset) = {
			let physical_addr = usize::try_from(io_apic_address).expect("APIC addr too big");
			let lower_addr =  PhysicalAddress::<1>::new(physical_addr).align_down();
			let offset = physical_addr - lower_addr.addr;
			let upper_addr: PhysicalAddress<4096> = PhysicalAddress::<1>::new(physical_addr + size_of::<Registers>()).align_up();
			let actual_size = NonZero::<usize>::new(upper_addr - lower_addr).expect("Cannot map zero size physical region");
			let page_count = unsafe { NonZero::<usize>::new_unchecked(actual_size.get().div_ceil(4096)) };
			let config = Config::<Global>::new(page_count)
					.physical_location(Location::At(Frame::new(lower_addr)))
					.physical_allocator(&crate::hal::acpi::Allocator);
			(
				Mapping::new(config, crate::paging_codes::IOAPIC_REGISTERS).expect("Unable to create physical mapping"),
				offset,
			)
		};

		let mut ioapic = Ioapic {
			mmap,
			offset,
			entry_count: 0,
			gsi_base: Line(global_system_interrupt_base),
		};

		ioapic.entry_count = ioapic.version_register().max_redirection_entry() + 1;
		
		ioapic
	}

	fn registers(&self) -> *mut Registers {
		unsafe {
			self.mmap.virtual_valid_start().as_ptr()
			    .byte_add(self.offset)
			    .cast()
		}
	}

	fn version_register(&mut self) -> VersionRegister {
		let registers = self.registers();
		let val = unsafe {
			addr_of_mut!((*registers).select).store_io(0x1);
			addr_of_mut!((*registers).data).load_io()
		};
		VersionRegister(val)
	}

	fn size(&self) -> usize {
		self.entry_count.try_into().unwrap()
	}

	fn redirection_entry(&mut self, num: u32) -> RedirectionEntry<'_> {
		assert!(num < self.entry_count, "Invalid redirection entry");

		RedirectionEntry {
			ioapic: self,
			num,
		}
	}

	fn redirection_entry_for(&mut self, line: Line) -> RedirectionEntry<'_> {
		let num = u32::try_from(line.0 - self.gsi_base.0).expect("Unsupported line for this I/O APIC");
		self.redirection_entry(num)
	}

	fn read_register(&mut self, addr: u32) -> u32 {
		let registers = self.registers();
		unsafe {
			addr_of_mut!((*registers).select).store_io(addr);
			addr_of_mut!((*registers).data).load_io()
		}
	}

	fn write_register(&mut self, addr: u32, val: u32) {
		let registers = self.registers();
		unsafe {
			addr_of_mut!((*registers).select).store_io(addr);
			addr_of_mut!((*registers).data).store_io(val);
		}
	}
}

impl InterruptController for Spinlock<Ioapic> {
	fn route_line_to(&self, line: Line, vector: Vector) {
		let mut guard = self.lock();
		guard.redirection_entry_for(line)
				.update(|entry| {
					entry.mask = true;
					entry.destination = 0;
					entry.destination_mode = DestinationMode::Physical;
					match vector {
						NMI_EXTERNAL_VECTOR => {
							entry.delivery_mode = DeliveryMode::Nmi;
							entry.vector = 0;
						},
						vector => {
							assert!((16..=255).contains(&vector.get()), "Vector {vector:?} invalid for interrupt");
							entry.delivery_mode = DeliveryMode::Fixed;
							entry.vector = vector.get() as u8;
						},
					}
				});
	}

	fn unmask_line(&self, line: Line) {
		let mut guard = self.lock();
		guard.redirection_entry_for(line)
		     .update(|entry| {
			     entry.mask = false;
		     });
	}

	fn mask_line(&self, line: Line) {
		let mut guard = self.lock();
		guard.redirection_entry_for(line)
		     .update(|entry| {
			     entry.mask = true;
		     });
	}

	fn eoi(&self, _line: Line) {
		todo!()
	}
}

#[repr(C)]
struct Registers {
	select: u32,
	_pad: [u32; 3],
	data: u32,
}

pub struct RedirectionEntry<'ioapic> {
	ioapic: &'ioapic mut Ioapic,
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

impl RedirectionEntry<'_> {
	pub fn update(&mut self, f: impl FnOnce(&mut EntryData)) {
		let low_register = 0x10 + self.num*2;
		let high_register = 0x10 + self.num*2 + 1;

		let low = self.ioapic.read_register(low_register);
		let high = self.ioapic.read_register(high_register);

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

		self.ioapic.write_register(low_register, new_low);
		self.ioapic.write_register(high_register, new_high);
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
