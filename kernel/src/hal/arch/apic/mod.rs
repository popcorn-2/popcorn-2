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
use crate::hal::timing::Timer;
use bit_field::BitField;
use crate::hal;

mod register;
mod timer;

use register::{Register, Allow, Deny};

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
