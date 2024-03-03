use core::arch::asm;
use core::arch::x86_64::CpuidResult;
use core::cell::{OnceCell, RefCell, UnsafeCell};
use core::fmt::Debug;
use core::mem;
use core::ptr::addr_of_mut;
use core::time::Duration;
use acpi::madt::MadtEntry;
use acpi::{AcpiHandler, PhysicalMapping};
use log::{debug, info};
use crate::hal::timing::{Eoi, Timer};
use bit_field::BitField;
use crate::hal;

mod register;
mod timer;

use register::{Register, Allow, Deny};
use timer::TimerMode;
use crate::threading::scheduler::IrqCell;

#[repr(C)]
pub struct Apic {
	_res0: [Register; 2],
	id: Register<Allow, Allow>,
	version: Register<Allow, Deny>,
	_res1: [Register; 4],
	task_priority: Register<Allow, Allow>,
	arbitration_priority: Register<Allow, Deny>,
	processor_priority: Register<Allow, Deny>,
	eoi: Register<Deny, Allow>,
	remote_read: Register<Allow, Deny>,
	logical_destination: Register<Allow, Allow>,
	destination_format: Register<Allow, Allow>,
	spurious_vector: Register<Allow, Allow>,
	_for_later: [Register; 34],
	timer_lvt: Register<Allow, Allow, timer::Lvt>,
	thermal_sensor_lvt: Register<Allow, Allow>,
	perf_monitor_lvt: Register<Allow, Allow>,
	lint0_lvt: Register<Allow, Allow>,
	lint1_lvt: Register<Allow, Allow>,
	error_lvt: Register<Allow, Allow>,
	timer_initial_count: Register<Allow, Allow>,
	timer_current_count: Register<Allow, Deny>,
	_res2: [Register; 4],
	timer_divide_config: Register<Allow, Allow>,
}

impl Apic {
	pub unsafe fn eoi(self: *mut Self) {
		unsafe { addr_of_mut!((*self).eoi).write_register(0) }
	}
}

struct Lapic(OnceCell<IrqCell<PhysicalMapping<hal::acpi::Handler<'static>, Apic>>>, UnsafeCell<()>);

#[thread_local]
static LAPIC: Lapic = Lapic(OnceCell::new(), UnsafeCell::new(()));

pub type LapicTimer = &'static IrqCell<PhysicalMapping<hal::acpi::Handler<'static>, Apic>>;

#[derive(Debug)]
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
		unsafe {
			let val = addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).read_register()
					.with_vector(irq.try_into().expect("Invalid vector"));
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).write_register(val);
		}
		Ok(())
	}

	fn get_time_period_picos(&self) -> Result<u64, SupportError> {
		let Ok(hpet) = acpi::hpet::HpetInfo::new(hal::acpi::tables()) else {
			return Err(SupportError::NoFreq);
		};

		let borrow = self.lock();
		let (start, end, hpet_period) = unsafe {
			let hpet = hal::acpi::Handler::new(&hal::acpi::Allocator).map_physical_region::<super::hpet::Header>(hpet.base_address, mem::size_of::<super::hpet::Header>());

			let old_val = addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).read_register();
			let val = old_val
			        .with_mode(TimerMode::OneShot)
					.with_mask(true);
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).write_register(val);

			let old_divide = addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_divide_config).read_register();
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_divide_config).write_register(0); // div by 2

			let hpet_config_old = addr_of_mut!((*hpet.virtual_start().as_ptr()).configuration).read_volatile();
			addr_of_mut!((*hpet.virtual_start().as_ptr()).configuration).write_volatile(hpet_config_old | 1);

			let hpet_start_count = addr_of_mut!((*hpet.virtual_start().as_ptr()).counter).read_volatile();

			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_initial_count).write_register(1_000_000);
			while addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_current_count).read_register() != 0 {}

			let hpet_end_count = addr_of_mut!((*hpet.virtual_start().as_ptr()).counter).read_volatile();
			addr_of_mut!((*hpet.virtual_start().as_ptr()).configuration).write_volatile(hpet_config_old);

			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).write_register(old_val);
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_divide_config).write_register(old_divide);

			let hpet_capabilities = addr_of_mut!((*hpet.virtual_start().as_ptr()).capabilities).read_volatile()
					.get_bits(32..=63);

			(hpet_start_count, hpet_end_count, hpet_capabilities)
		};

		let period_femptoseconds = (end - start) * hpet_period / 2_000_000;

		Ok(period_femptoseconds / 1000)
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
		unsafe {
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_divide_config).write_register(val);
		}
		Ok(())
	}

	fn set_oneshot_time(&mut self, ticks: u128) -> Result<(), <u32 as TryFrom<u128>>::Error> {
		let borrow = self.lock();
		unsafe {
			let val = addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).read_register()
			                                            .with_mode(TimerMode::OneShot)
			                                            .with_mask(false);
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).write_register(val);
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_initial_count).write_register(ticks.try_into()?);
		}
		Ok(())
	}

	fn start_periodic(&mut self, ticks: u128) -> Result<(), <u32 as TryFrom<u128>>::Error> {
		let borrow = self.lock();
		unsafe {
			let val = addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).read_register()
																				.with_mode(TimerMode::Periodic)
																				.with_mask(false);
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_lvt).write_register(val);
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_initial_count).write_register(ticks.try_into()?);
		}
		Ok(())
	}

	fn stop_periodic(&mut self) {
		let borrow = self.lock();
		unsafe {
			addr_of_mut!((*borrow.virtual_start().as_ptr()).timer_initial_count).write_register(0);
		}
	}

	fn eoi_handle(&mut self) -> EoiHandle {
		EoiHandle(*self)
	}
}

#[derive(Clone, Copy)]
pub struct EoiHandle(LapicTimer);

impl Eoi for EoiHandle {
	fn send(self) {
		unsafe { self.0.lock().virtual_start().as_ptr().eoi(); }
	}
}

pub(in crate::hal) fn init(spurious_vector: u8) {
	let Ok(madt) = hal::acpi::tables().find_table::<::acpi::madt::Madt>() else {
		panic!("No MADT found");
	};

	let mut apic_addr = madt.local_apic_address as u64;

	for entry in madt.entries() {
		match entry {
			MadtEntry::LocalApicAddressOverride(addr) => {
				apic_addr = addr.local_apic_address;
			}
			_ => {}
		}
	}

	let apic = unsafe { hal::acpi::Handler::new(&hal::acpi::Allocator).map_physical_region::<Apic>(apic_addr as usize, mem::size_of::<Apic>()) };

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

		let val = addr_of_mut!((*apic.virtual_start().as_ptr()).spurious_vector).read_register();
		debug!("current apic spv {val:#x}");

		let val = (val & !0xFF) | u32::from(spurious_vector) | 0x100;
		addr_of_mut!((*apic.virtual_start().as_ptr()).spurious_vector).write_register(val);

		let val = addr_of_mut!((*apic.virtual_start().as_ptr()).timer_lvt).read_register()
				.with_mask(false);
		addr_of_mut!((*apic.virtual_start().as_ptr()).timer_lvt).write_register(val);
	}

	LAPIC.0.get_or_init(|| IrqCell::new(apic));
}
