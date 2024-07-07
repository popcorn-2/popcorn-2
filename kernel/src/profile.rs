use core::arch::asm;
use crate::prelude::*;
use core::arch::x86_64::{__cpuid, CpuidResult};
use core::fmt::{Display, Formatter, Write};
use kernel_api::time::Instant;
use core::sync::atomic::{AtomicU128, AtomicU32, Ordering};
use bit_field::BitField;

const NAME: &'static str = "kernel";
const PID: usize = 1;
const EVENT_NAME: &'static str = "cycles:Pk";

mod msr {
	use core::arch::asm;

	pub const IA32_FIXED_CTR_CTRL: usize = 0x38D;
	pub const IA32_PERF_GLOBAL_CTRL: usize = 0x38F;
	pub const IA32_FIXED_CTR0: usize = 0x309;
	pub const IA32_PERF_GLOBAL_STATUS: usize = 0x38E;
	pub const IA32_PERF_GLOBAL_OVF_CTL: usize = 0x390;

	pub fn wrmsr(msr: usize, value: u64) {
		let high = (value >> 32) as u32;
		let low = value as u32;

		unsafe {
			asm!("wrmsr", in("rcx") msr, in("eax") low, in("edx") high);
		}
	}

	pub fn rdmsr(msr: usize) -> u64 {
		let high: u32;
		let low: u32;

		unsafe {
			asm!("rdmsr", in("rcx") msr, out("eax") low, out("edx") high);
		}

		(high as u64) << 32 | (low as u64)
	}
}

static LAST_SAMPLE: AtomicU128 = AtomicU128::new(0);
static PMC_MASK: AtomicU128 = AtomicU128::new(0);

fn intel_reset() {
	// disable counter
	msr::wrmsr(msr::IA32_FIXED_CTR_CTRL, 0);

	// set to generate every 1 sec @ 3.1 GHz
	msr::wrmsr(msr::IA32_FIXED_CTR0, (1u64 << 48) - 5000000);

	// clear overflow state
	msr::wrmsr(msr::IA32_PERF_GLOBAL_OVF_CTL, 1 << 32);

	// enable counter
	msr::wrmsr(msr::IA32_FIXED_CTR_CTRL, 1 << 3 | 1); // generate interrupt and only count in ring 0
}

/*fn reset_pmc0_intel() {
	{
		let val: u32 = (0 << 22) | // Disable
				(1 << 20) | // Interrupts
				(1 << 17) | // OS
				0x3C; // Core cycle mask
		unsafe {
			asm!("wrmsr", in("rcx") 0x186, in("eax") val, in("edx") 0);
		}
	}

	{
		//let low: u32;
		//let high: u32;
		unsafe {
			asm!("wrmsr", in("rcx") 0x390, in("eax") 1, in("edx") 0);
		}
	}

	let val = (1u64 << 48) - 15500000000; // every 5 seconds @ 3.1 GHz
	let low = val as u32;
	let high = (val >> 32) as u32;
	unsafe {
		asm!("wrmsr", in("rcx") 0x0C1, in("eax") low, in("edx") high);
	}

	{
		let val: u32 = (1 << 22) | // Enable
				(1 << 20) | // Interrupts
				(1 << 17) | // OS
				0x3C; // Core cycle mask
		unsafe {
			asm!("wrmsr", in("rcx") 0x186, in("eax") val, in("edx") 0);
		}
	}
}*/

pub fn did_overflow() -> bool {
	msr::rdmsr(msr::IA32_PERF_GLOBAL_STATUS) & (1 << 32) != 0
}

pub fn intel_handle(mut to: impl Write) {
	if !did_overflow() {
		let _ = writeln!(to, "PMC did not overflow");
		return;
	}

	let now = crate::timing::tsc();
	let prev = LAST_SAMPLE.swap(now, Ordering::Relaxed);
	let (seconds, micros) = {
		let (num, denom) = crate::timing::tsc_to_nanos();
		let nanos = now * num / denom.get();
		let micros = nanos / 1000;
		(micros / 1_000_000, micros % 1_000_000)
	};
	let sample_rate = now - prev;

	let _ = writeln!(&mut to, "{NAME} {PID} {seconds}.{micros}: {sample_rate: >11} {EVENT_NAME}:");
	let mut out_of_irq_bit = false;
	crate::panicking::stack_trace_iter(|ip, sym| {
		if out_of_irq_bit {
			let _ = writeln!(&mut to, "{}{ip: >24x} {} ({})", if out_of_irq_bit { "" } else { "-- " }, sym.name, sym.file);
		} else if sym.name == "amd64_global_irq_handler" {
			out_of_irq_bit = true;
		}
	});
	let _ = writeln!(&mut to);

	intel_reset();
}

#[repr(u32)]
enum Event {
	CoreCycle = 0,
	InstructionRetired,
	ReferenceCycle,
	LastLevelCacheReference,
	LastLevelCacheMiss,
	BranchRetired,
	BranchMispredictRetired,
	TopDownSlots,
}

pub fn init_intel() {
	let CpuidResult { eax, ebx, .. } = unsafe {__cpuid(0xa) };
	let version = eax.get_bits(0..=7);
	let counter_count = eax.get_bits(8..=15);
	let counter_width = eax.get_bits(16..=23);
	let event_bitvec_size = eax.get_bits(24..=31);
	let event_supported = |event: Event| {
		let event = event as u32;
		event_bitvec_size > event && (ebx & event) == 0
	};
	sprintln!(
		"Perf monitoring support:
		\tversion: {version}
		\t{counter_count} event counters
		\t{counter_width} bits per counter
		\tCycle events supported: {}
		\tInstruction retired events supported: {}
		\tReference cycle events supported: {}
		\tLast level cache reference events supported: {}
		\tLast level cache miss events supported: {}
		\tBranch instruction retired events supported: {}
		\tBranch instruction mispredict events supported: {}
		\tTop-down slots events supported: {}
		",
		event_supported(Event::CoreCycle),
		event_supported(Event::InstructionRetired),
		event_supported(Event::InstructionRetired),
		event_supported(Event::ReferenceCycle),
		event_supported(Event::LastLevelCacheReference),
		event_supported(Event::LastLevelCacheMiss),
		event_supported(Event::BranchRetired),
		event_supported(Event::BranchMispredictRetired),
	);

	assert!(version >= 2);
	assert!(event_supported(Event::CoreCycle));
	assert!(counter_width <= 128);
	PMC_MASK.store((1 << counter_width) - 1, Ordering::Relaxed);

	// disable all counters
	msr::wrmsr(msr::IA32_FIXED_CTR_CTRL, 0);

	// enable fixed counter 0
	msr::wrmsr(msr::IA32_PERF_GLOBAL_CTRL, 1 << 32);

	intel_reset();
}
