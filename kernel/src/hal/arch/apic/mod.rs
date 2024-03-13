use core::arch::asm;
use core::arch::x86_64::CpuidResult;
use core::cell::{OnceCell, RefCell, UnsafeCell};
use core::fmt::Debug;
use core::mem;
use core::ptr::addr_of_mut;
use core::time::Duration;
use acpi::madt::MadtEntry;
use acpi::{AcpiHandler, PhysicalMapping};
use log::{debug, info, warn};
use crate::hal::timing::{Eoi, Timer};
use bit_field::BitField;
use kernel_api::sync::OnceLock;
use macros::Fields;
use crate::hal;
use timer::TimerMode;
use crate::hal::arch::apic::ioapic::{ActiveLevel, Ioapics, LegacyMap, TriggerMode};
use crate::mmio::MmioCell;
use crate::threading::scheduler::IrqCell;
use crate::projection::Project;

mod timer;
mod ioapic;

macro_rules! apic_register_ty {
    () => {u32};
	($field_ty:path) => {$field_ty}
}

macro_rules! apic_registers {
    ($(#[$attr:meta])* $vis:vis struct $name:ident { $($field_vis:vis $field:ident $(: $field_ty:path)?),* $(,)? }) => {
	    paste::paste! {
		    $(#[$attr])* $vis struct $name {
			    $(
			        $field_vis $field: apic_register_ty!($($field_ty)?),
			        [<_pad_ $field>]: [u32; 3],
			    )*
		    }
		}
    };
}

apic_registers! {
	#[derive(Fields)]
	#[repr(C)]
	pub struct Apic {
		_res0,
		_res1,
		id,
		version,
		_res2,
		_res3,
		_res4,
		_res5,
		task_priority,
		arbitration_priority,
		processor_priority,
		eoi,
		remote_read,
		logical_destination,
		destination_format,
		spurious_vector,
		_res6,
		_res7,
		_res8,
		_res9,
		_res10,
		_res11,
		_res12,
		_res13,
		_res14,
		_res15,
		_res16,
		_res17,
		_res18,
		_res19,
		_res20,
		_res21,
		_res22,
		_res23,
		_res24,
		_res25,
		_res26,
		_res27,
		_res28,
		_res29,
		_res30,
		_res31,
		_res32,
		_res33,
		_res34,
		_res35,
		_res36,
		_res37,
		_res38,
		_res39,
		timer_lvt: timer::Lvt,
		thermal_sensor_lvt,
		perf_monitor_lvt,
		lint0_lvt,
		lint1_lvt,
		error_lvt,
		timer_initial_count,
		timer_current_count,
		_res40,
		_res41,
		_res42,
		_res43,
		timer_divide_config,
	}
}

impl MmioCell<Apic> {
	pub unsafe fn eoi(&mut self) {
		self.project::<Apic::eoi>().write(0);
	}
}

struct Lapic(OnceCell<IrqCell<PhysicalMapping<hal::acpi::Handler<'static>, Apic>>>, UnsafeCell<()>);

#[thread_local]
static LAPIC: Lapic = Lapic(OnceCell::new(), UnsafeCell::new(()));

pub type LapicTimer = &'static IrqCell<PhysicalMapping<hal::acpi::Handler<'static>, Apic>>;

#[derive(Debug, Copy, Clone)]
pub enum SupportError {
	NoLeaf(u32),
	NoFreq
}

impl Timer for LapicTimer {
	fn get() -> Self {
		// FIXME: This is invalid if TLS section can be freed (CPU hotplug???)
		let r = LAPIC.0.get().expect("ACPI initialisation not done");
		unsafe { mem::transmute(r) }
	}

	fn set_irq_number(&mut self, irq: usize) -> Result<(), ()> {
		let borrow = self.lock();
		let apic = unsafe { MmioCell::new(borrow.virtual_start().as_ptr()) };

		let mut lvt = apic.project::<Apic::timer_lvt>();
		let val = lvt.read()
				.with_vector(irq.try_into().expect("Invalid vector"));
		lvt.write(val);

		Ok(())
	}

	fn get_time_period_picos(&self) -> Result<u64, SupportError> {
		static CACHED_TIME: OnceLock<Result<u64, SupportError>> = OnceLock::new();

		*CACHED_TIME.get_or_init(|| {
			let Ok(hpet) = acpi::hpet::HpetInfo::new(hal::acpi::tables()) else {
				return Err(SupportError::NoFreq);
			};

			let borrow = self.lock();
			let apic = unsafe { MmioCell::new(borrow.virtual_start().as_ptr()) };

			let (start, end, hpet_period) = {
				use super::hpet::Header as Hpet;

				let hpet = unsafe { hal::acpi::Handler::new(&hal::acpi::Allocator).map_physical_region::<Hpet>(hpet.base_address, mem::size_of::<super::hpet::Header>()) };
				let hpet = unsafe { MmioCell::new(hpet.virtual_start().as_ptr()) };

				let mut timer_lvt = apic.project::<Apic::timer_lvt>();
				let mut timer_divide_register = apic.project::<Apic::timer_divide_config>();
				let mut timer_initial_count = apic.project::<Apic::timer_initial_count>();
				let timer_current_count = apic.project::<Apic::timer_current_count>();

				let old_val = timer_lvt.read();
				let val = old_val
						.with_mode(TimerMode::OneShot)
						.with_mask(true);
				timer_lvt.write(val);

				let old_divide = timer_divide_register.read();
				timer_divide_register.write(0); // div by 2

				let mut hpet_config_register = hpet.project::<Hpet::configuration>();
				let hpet_config_old = hpet_config_register.read();
				hpet_config_register.write(hpet_config_old.set_enabled(true));

				let hpet_counter = hpet.project::<Hpet::counter>();
				let hpet_start_count = hpet_counter.read();

				timer_initial_count.write(1_000_000);
				while timer_current_count.read() != 0 {}

				let hpet_end_count = hpet_counter.read();

				hpet_config_register.write(hpet_config_old);
				timer_lvt.write(old_val);
				timer_divide_register.write(old_divide);

				let hpet_capabilities = hpet.project::<Hpet::capabilities>().read().period_femtoseconds();

				(hpet_start_count, hpet_end_count, hpet_capabilities)
			};

			let period_femtoseconds = (end - start) * hpet_period / 2_000_000;

			Ok(period_femtoseconds / 1000)
		})
	}

	fn get_divisors(&self) -> impl IntoIterator<Item=u64> {
		[1, 2, 4, 8, 16, 32, 64, 128]
	}

	fn set_divisor(&mut self, divisor: u64) -> Result<(), ()> {
		let val = match divisor {
			1   => 0b1011,
			2   => 0b0000,
			4   => 0b0001,
			8   => 0b0010,
			16  => 0b0011,
			32  => 0b1000,
			64  => 0b1001,
			128 => 0b1010,
			_ => return Err(())
		};

		let borrow = self.lock();
		let apic = unsafe { MmioCell::new(borrow.virtual_start().as_ptr()) };
		apic.project::<Apic::timer_divide_config>().write(val);
		Ok(())
	}

	fn set_oneshot_time(&mut self, ticks: u128) -> Result<(), <u32 as TryFrom<u128>>::Error> {
		let borrow = self.lock();
		let apic = unsafe { MmioCell::new(borrow.virtual_start().as_ptr()) };

		let val = apic.project::<Apic::timer_lvt>().read()
		                                            .with_mode(TimerMode::OneShot)
		                                            .with_mask(false);
		apic.project::<Apic::timer_lvt>().write(val);
		apic.project::<Apic::timer_initial_count>().write(ticks.try_into()?);

		Ok(())
	}

	fn start_periodic(&mut self, ticks: u128) -> Result<(), <u32 as TryFrom<u128>>::Error> {
		let borrow = self.lock();
		let apic = unsafe { MmioCell::new(borrow.virtual_start().as_ptr()) };

		let val = apic.project::<Apic::timer_lvt>().read()
		                     .with_mode(TimerMode::Periodic)
		                     .with_mask(false);
		apic.project::<Apic::timer_lvt>().write(val);
		apic.project::<Apic::timer_initial_count>().write(ticks.try_into()?);

		Ok(())
	}

	fn stop_periodic(&mut self) {
		let borrow = self.lock();
		let apic = unsafe { MmioCell::new(borrow.virtual_start().as_ptr()) };
		apic.project::<Apic::timer_initial_count>().write(0);
	}

	fn eoi_handle(&mut self) -> EoiHandle {
		EoiHandle(*self)
	}
}

#[derive(Clone, Copy)]
pub struct EoiHandle(LapicTimer);

impl Eoi for EoiHandle {
	fn send(self) {
		unsafe { MmioCell::new(self.0.lock().virtual_start().as_ptr()).eoi(); }
	}
}

pub(in crate::hal) fn init(spurious_vector: u8) {
	let Ok(madt) = hal::acpi::tables().find_table::<::acpi::madt::Madt>() else {
		panic!("No MADT found");
	};

	let mut apic_addr = madt.local_apic_address as u64;
	let mut ioapics = Ioapics::new();
	let mut legacy_gsi_mapping = LegacyMap::pc_default();

	for entry in madt.entries() {
		match entry {
			MadtEntry::LocalApicAddressOverride(addr) => {
				apic_addr = addr.local_apic_address;
			}
			MadtEntry::IoApic(ioapic) => {
				let gsi = ioapic.global_system_interrupt_base as usize;
				let addr = ioapic.io_apic_address;
				let ioapic = unsafe { ioapic::Ioapic::new(addr as usize, hal::acpi::Handler::new(&hal::acpi::Allocator)) };
				debug!("Found I/O APIC with GSIs {} -> {} at {:#x}", gsi, gsi + ioapic.size(), addr);
				ioapics.push(gsi, ioapic);
			}
			MadtEntry::InterruptSourceOverride(iso) => {
				if iso.bus != 0 { warn!("Unknown interrupt bus: {iso:?}"); }
				else {
					let level = if iso.flags & 2 == 0 { ActiveLevel::High } else { ActiveLevel::Low };
					let mode = if iso.flags & 8 == 0 { TriggerMode::Edge } else { TriggerMode::Level };
					let entry = (iso.global_system_interrupt, mode, level);

					match iso.irq {
						0 => legacy_gsi_mapping.pit = entry,
						1 => legacy_gsi_mapping.ps2_keyboard = entry,
						3 => legacy_gsi_mapping.com2 = entry,
						4 => legacy_gsi_mapping.com1 = entry,
						5 => legacy_gsi_mapping.lpt2 = entry,
						6 => legacy_gsi_mapping.floppy = entry,
						8 => legacy_gsi_mapping.rtc = entry,
						12 => legacy_gsi_mapping.ps2_mouse = entry,
						14 => legacy_gsi_mapping.ata_primary = entry,
						15 => legacy_gsi_mapping.ata_secondary = entry,
						irq => warn!("Unused GSI override: {irq}={entry:?}"),
					}
				}
			}
			_ => {}
		}
	}

	debug!("I/O APICs: {:?}", ioapics);
	debug!("Legacy mapping: {:?}", legacy_gsi_mapping);

	let apic = unsafe { hal::acpi::Handler::new(&hal::acpi::Allocator).map_physical_region::<Apic>(apic_addr as usize, mem::size_of::<Apic>()) };
	let apic_boxed = unsafe { MmioCell::new(apic.virtual_start().as_ptr()) };

	info!("LAPIC located at {apic_addr:#x}");

	unsafe {
		let val = {
			let low: u32;
			let high: u32;

			asm!("rdmsr", in("ecx") 0x1B, out("rax") low, out("rdx") high);
			(high as u64) << 32 | (low as u64)
		};
		debug!("current apic base {val:#x}");
		let new = val | 0x800;
		let new = (new as u32, (new >> 32) as u32);
		asm!("wrmsr", in("ecx") 0x1B, in("rax") new.0, in("rdx") new.1);
	}

	{
		debug!("{apic_boxed:p}");
		let mut spurious_vector_register = apic_boxed.project::<Apic::spurious_vector>();
		let val = spurious_vector_register.read();
		debug!("current apic spv {:#x}", val);

		let val = (val & !0xFF) | u32::from(spurious_vector) | 0x100;
		spurious_vector_register.write(val);
	}

	let mut timer_lvt = apic_boxed.project::<Apic::timer_lvt>();
	let val = timer_lvt.read()
		.with_mask(false);
	timer_lvt.write(val);

	LAPIC.0.get_or_init(|| IrqCell::new(apic));
}
