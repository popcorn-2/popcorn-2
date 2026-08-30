use alloc::sync::Arc;
use core::num::NonZero;
use core::pin::Pin;
use core::ptr::addr_of_mut;
use core::time::Duration;
use acpi::sdt::madt::{Madt, MadtEntry};
use kernel_api::address_space::Kernel;
use kernel_api::is_x86_feature_detected;
use kernel_api::mapping::{Caching, Config, Mapping, Mmap, Ty};
use kernel_api::memory::PhysicalAddress;
use kernel_api::time::Instant;
use crate::hal;
use crate::hal::arch::apic::lapic::Registers;
use crate::hal::arch::amd64::msr;
use crate::hal::interrupts_v2::Vector;
use crate::hal::timing::{Timer, TimerMeta};

pub(in crate::hal) struct XApicInner {
	mmap: Arc<Mapping<Mmap, Kernel>>,
	offset: usize,
}

pub(super) struct XApic(#[expect(dead_code)] XApicInner);
pub(in crate::hal) struct XApicTimer(pub(in crate::hal) XApicInner, u32);

impl XApic {
	pub(super) fn init(madt: Pin<&Madt>) {
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
			let physical_addr = PhysicalAddress::new(usize::try_from(apic_addr).expect("APIC addr too big"));
			let lower_addr = physical_addr.align_down_to_frame();
			let offset = physical_addr - *lower_addr;
			let upper_addr = (physical_addr + size_of::<Registers>()).align_up_to_frame();
			let page_count = NonZero::<usize>::new(upper_addr - lower_addr).expect("Cannot map zero size physical region");
			
			let mmap = Config::new(page_count, Ty::APIC_REGISTERS)
					.physical_location(lower_addr)
					.with_allocator(&hal::acpi::Allocator)
					.caching(Caching::Mmio)
					.protection(true, false, false)
					.map()
					.expect("Unable to create physical mapping");
			
			(mmap, offset)
		};

		let mut xapic = XApicInner { mmap: Arc::new(mmap), offset };

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

		let ticks_per_50ms = if is_x86_feature_detected!("tsc_deadline") {
			unsafe {
				let _ = addr_of_mut!((*xapic.registers()).timer_lvt.0.0).store_io(
					// TSC-deadline, unmasked, vector 0x40
					0x40040
				);
			}
			0
		} else {
			info!("No `tsc-deadline` - fall back to APIC one-shot - calibrating...");
			unsafe {
				addr_of_mut!((*xapic.registers()).timer_lvt.0.0).store_io(
					// one-shot, masked, vector 0x40
					0x10040
				);
				addr_of_mut!((*xapic.registers()).timer_divide_config).store_io(0b11); // div by 16

				// time 50ms and see how far LAPIC timer ticks down
				let start = Instant::now();
				addr_of_mut!((*xapic.registers()).timer_initial_count).store_io(0xFFFFFFFF);
				while (Instant::now() - start) < Duration::from_millis(50) {};
				let counted = 0xFFFFFFFF - addr_of_mut!((*xapic.registers()).timer_current_count).load_io();
				addr_of_mut!((*xapic.registers()).timer_initial_count).store_io(0);
				addr_of_mut!((*xapic.registers()).timer_lvt.0.0).store_io(
					// one-shot, unmasked, vector 0x40
					0x00040
				);
				counted
			}
		};

		let xapic_timer = TimerMeta::new(Vector(0x40), Box::leak(Box::new(XApicTimer(xapic, ticks_per_50ms))));
		hal::timing::init_local_timer(xapic_timer);
	}
}

impl XApicInner {
	fn registers(&mut self) -> *mut Registers {
		unsafe {
			self.mmap.as_ptr()
				.cast_mut()
			    .byte_add(self.offset)
			    .cast()
		}
	}

	#[expect(dead_code)]
	pub(in crate::hal) fn eoi(&mut self, _vector: Vector) {
		let registers = self.registers();
		unsafe {
			addr_of_mut!((*registers).eoi).store_io(0);
		}
	}
}

impl Timer for XApicTimer {
	fn mask(&mut self, _masked: bool) {
		/*let registers = self.0.registers();
		unsafe {
			let _ = addr_of_mut!((*registers).timer_lvt.0.0).fetch_update_io(|mut old| {
				Some(
					*old.set_bit(16, masked) // masked
				)
			});
		}*/
	}

	fn set_deadline(&mut self, time: Instant) -> Result<(), ()> {
		if is_x86_feature_detected!("tsc_deadline") {
			info!("Set TSC for {time:?}");
			msr::wrmsr(msr::IA32_TSC_DEADLINE, time.get().try_into().map_err(|_| ())?);
		} else {
			info!("Set APIC one-shot for {time:?}");
			if let Some(delta) = time.checked_duration_since(Instant::now()) && !delta.is_zero() {
				let ticks = delta.as_millis() * (self.1 as u128) / 50;
				unsafe {
					addr_of_mut!((*self.0.registers()).timer_initial_count).store_io(ticks.try_into().map_err(|_| ())?);
				}
			} else {
				warn!("ignoring one-shot request in the past");
			}
		}
		Ok(())
	}
}
