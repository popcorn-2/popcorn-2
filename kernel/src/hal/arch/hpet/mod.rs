use core::fmt::{Debug, Formatter};
use core::mem;
use core::ptr::addr_of_mut;
use acpi::AcpiHandler;
use macros::Fields;
use crate::hal::acpi::{AcpiHandlerExt, Handler, XPhysicalMapping};
use crate::mmio::MmioCell;
use crate::projection::Project;
use bit_field::BitField;
use log::debug;

mod timer;

use timer::Timer;

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Capabilities(u64);

impl Capabilities {
	pub fn period_femtoseconds(self) -> u64 {
		self.0.get_bits(32..64)
	}

	pub fn vendor(self) -> u16 {
		self.0.get_bits(16..32).try_into().unwrap()
	}

	pub fn legacy_mapping_capable(self) -> bool {
		self.0.get_bit(15)
	}

	pub fn counter_64_bit_capable(self) -> bool {
		self.0.get_bit(13)
	}

	pub fn timer_count(self) -> usize {
		usize::try_from(self.0.get_bits(8..13)).unwrap() + 1
	}
}

impl Debug for Capabilities {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Capabilities")
				.field("period_femtoseconds", &self.period_femtoseconds())
				.field("vendor", &self.vendor())
				.field("legacy_mapping_capable", &self.legacy_mapping_capable())
				.field("64_bit_capable", &self.counter_64_bit_capable())
				.field("timer_count", &self.timer_count())
				.finish_non_exhaustive()
	}
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Configuration(u64);

impl Configuration {
	pub fn legacy_mapping_enabled(self) -> bool {
		self.0.get_bit(1)
	}

	pub fn enabled(self) -> bool {
		self.0.get_bit(0)
	}

	pub fn set_legacy_mapping_enabled(mut self, enabled: bool) -> Self {
		Configuration(*self.0.set_bit(1, enabled))
	}

	pub fn set_enabled(mut self, enabled: bool) -> Self {
		Configuration(*self.0.set_bit(0, enabled))
	}
}

impl Debug for Configuration {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("Configuration")
		 .field("enabled", &self.enabled())
		 .field("legacy_mapping_enabled", &self.legacy_mapping_enabled())
		 .finish_non_exhaustive()
	}
}

#[repr(C)]
#[derive(Fields, Debug, Copy, Clone)]
pub struct Header {
	pub(super) capabilities: Capabilities,
	_res0: u64,
	pub(super) configuration: Configuration,
	_res1: u64,
	pub(super) status: u64,
	_res2: [u64; 25],
	pub(super) counter: u64,
	_res3: u64,
}

#[repr(C)]
pub struct HpetInner {
	header: Header,
	timers: [Timer]
}

impl Debug for MmioCell<HpetInner> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mut d = f.debug_struct("HpetInner");
		d.field(
			"header",
			&unsafe { addr_of_mut!((*self.as_ptr()).header).read_volatile() }
		);

		struct TimerDebugHelper(*mut [Timer]);

		impl Debug for TimerDebugHelper {
			fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
				let mut d = f.debug_list();

				for idx in 0..self.0.len() {
					let timer = unsafe { self.0.get_unchecked_mut(idx).read_volatile() };
					d.entry(&timer);
				}

				d.finish()
			}
		}

		d.field("timers", &TimerDebugHelper(unsafe { addr_of_mut!((*self.as_ptr()).timers) }));

		d.finish()
	}
}

pub struct Hpet<H: AcpiHandlerExt> {
	cell: MmioCell<HpetInner>,
	map: XPhysicalMapping<H, HpetInner>,
}

impl<H: AcpiHandlerExt> Hpet<H> {
	pub unsafe fn init(hpet: acpi::hpet::HpetInfo, handler: H) -> Self {
		let map = unsafe { handler.map_region::<Header>(hpet.base_address, mem::size_of::<Header>(), ()) };
		let cell = unsafe { MmioCell::new(map.virtual_start.as_ptr()) };
		let header = cell.read();
		
		let timer_count = header.capabilities.timer_count();
		let hpet_size = mem::size_of::<Header>() + timer_count*mem::size_of::<Timer>();
		drop(map);
		let map = unsafe { handler.map_region::<HpetInner>(hpet.base_address, hpet_size, timer_count) };
		let cell = unsafe { MmioCell::new(map.virtual_start.as_ptr()) };

		debug!("HPET: {cell:#?}");
		
		Self {
			cell,
			map
		}
	}
}
