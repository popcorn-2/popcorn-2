use alloc::sync::Arc;
use core::num::NonZero;
use crate::prelude::*;
use core::ptr::{addr_of, addr_of_mut};
use acpi::madt::{Madt, MadtEntry};
use bit_field::BitField;
use kernel::hal::arch::apic::lapic::Lvt;
use kernel_api::memory::mapping::{Config, Location, Mapping};
use kernel_api::memory::{Frame, PhysicalAddress};
use kernel_api::memory::r#virtual::Global;
use kernel_api::time::Instant;
use crate::hal;
use crate::hal::arch::apic::lapic::{DeliveryMode, LvtState, Registers};
use crate::hal::arch::amd64::msr;
use crate::hal::interrupts_v2::Vector;
use crate::hal::timing::{Timer3, TimerMeta};

pub(in crate::hal) struct XApicInner {
	mmap: Arc<Mapping<'static>>,
	offset: usize,
}

pub(super) struct XApic(XApicInner);
pub(in crate::hal) struct XApicTimer(pub(in crate::hal) XApicInner);

impl XApic {
	pub(super) fn init(madt: &Madt) {
		let mut apic_addr = madt.local_apic_address as u64;
		for entry in madt.entries() {
			match entry {
				MadtEntry::LocalApicAddressOverride(addr) => {
					apic_addr = addr.local_apic_address;
				},
				_ => {},
			}
		}

		info!("xAPIC located at {apic_addr:#x}");

		let (mmap, offset) = {
			let physical_addr = usize::try_from(apic_addr).expect("APIC addr too big");
			let lower_addr = PhysicalAddress::<1>::new(physical_addr).align_down();
			let offset = physical_addr - lower_addr.addr;
			let upper_addr: PhysicalAddress<4096> = PhysicalAddress::<1>::new(physical_addr + size_of::<Registers>()).align_up();
			let actual_size = NonZero::<usize>::new(upper_addr - lower_addr).expect("Cannot map zero size physical region");
			let page_count = unsafe { NonZero::<usize>::new_unchecked(actual_size.get().div_ceil(4096)) };
			let config = Config::<Global>::new(page_count)
					.physical_location(Location::At(Frame::new(lower_addr)))
					.physical_allocator(&hal::acpi::Allocator);
			(
				Mapping::new(config, crate::paging_codes::APIC_REGISTERS).expect("Unable to create physical mapping"),
				offset,
			)
		};

		let xapic = XApicInner { mmap: Arc::new(mmap), offset };

		msr::wrmsr(
			msr::IA32_APIC_BASE,
			msr::rdmsr(msr::IA32_APIC_BASE) | 0x800
		);

		let spurious_vector = unsafe { addr_of_mut!((*xapic.registers()).spurious_vector) };
		unsafe {
			let val = spurious_vector.load_io();
			spurious_vector.store_io((val & !0xFF) | (hal::SPURIOUS_VECTOR.0 as u32) | 0x100);
		}

		let xapic_id = unsafe { addr_of_mut!((*xapic.registers()).id).load_io() };

		for entry in madt.entries() {
			match entry {
				MadtEntry::LocalApicNmi(nmi) => {
					if u32::from(nmi.processor_id) == xapic_id {
						debug!("Set LINT{} for NMI", nmi.nmi_line);
						todo!()
					}
				},
				_ => {},
			}
		}

		unsafe {
			let _ = addr_of_mut!((*xapic.registers()).timer_lvt.0.0).fetch_update_io(|mut old| {
				Some(
					*old.set_bits(17..=18, 0b10) // TSC deadline
						.set_bit(16, true) // masked
						.set_bits(0..=7, 0x40) // vector
				)
			});
		}

		let xapic_timer = TimerMeta::new(Vector(0x40), Box::leak(Box::new(XApicTimer(xapic))));
		hal::timing::init_local_timer(xapic_timer);
	}
}

impl XApicInner {
	fn registers(&self) -> *mut Registers {
		unsafe {
			self.mmap.virtual_start().as_ptr()
			    .byte_add(self.offset)
			    .cast()
		}
	}

	pub(in crate::hal) fn eoi(&self, _vector: Vector) {
		let registers = self.registers();
		unsafe {
			addr_of_mut!((*registers).eoi).store_io(0);
		}
	}
}

impl Timer3 for XApicTimer {
	fn mask(&self, masked: bool) {
		let registers = self.0.registers();
		unsafe {
			let _ = addr_of_mut!((*registers).timer_lvt.0.0).fetch_update_io(|mut old| {
				Some(
					*old.set_bit(16, masked) // masked
				)
			});
		}
	}

	fn set_deadline(&self, time: Instant) -> Result<(), ()> {
		debug!("Set TSC for {time:?}");
		msr::wrmsr(msr::IA32_TSC_DEADLINE, time.get().try_into().map_err(|_| ())?);
		Ok(())
	}
}
