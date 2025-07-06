use alloc::collections::BinaryHeap;
#[allow(unused_imports)] use crate::prelude::*;
use core::arch::asm;
use core::arch::x86_64::{__cpuid, CpuidResult};
use core::cmp::{Ordering, Reverse};
use core::num::NonZero;
use bit_field::BitField;
use kernel_api::is_x86_feature_detected;
use kernel_api::sync::{IrqCell, IrqGuard, OnceLock};
use kernel_api::time::Instant;
use crate::{hal, non_zero};
use crate::hal::timing::TimerMeta;
use crate::threading::{Waker, WakeReason, WakeTrigger};

static TSC_MULTIPLIER: OnceLock<(u128, NonZero<u128>)> = OnceLock::new();

#[inline(always)]
#[export_name = "__popcorn_system_time"]
pub(crate) fn tsc() -> u128 {
	let low: u32;
	let high: u32;
	unsafe {
		asm!("rdtsc", out("eax") low, out("edx") high, options(nostack, preserves_flags, nomem));
	}
	(low as u128) | (high as u128) << 32
}

#[export_name = "__popcorn_system_time_scale"]
pub(crate) fn tsc_to_nanos() -> (u128, NonZero<u128>) {
	*TSC_MULTIPLIER.get_or_init(|| {
		let mut multiplier = None::<(u128, NonZero<u128>)>;

		let max_leaf = unsafe { __cpuid(0) }.eax;
		if max_leaf >= 0x15 {
			debug!("[TSC] Leaf 15h supported");
			let CpuidResult { eax: denom, ebx: num, ecx: freq, .. } = unsafe { __cpuid(0x15) };
			if let Some(num) = NonZero::<u128>::new(num.into()) && let Some(freq) = NonZero::<u128>::new(freq.into()) {
				debug!("[TSC] Leaf 15h enumerated");
				multiplier = Some((1000000000 * u128::from(denom), num.checked_mul(freq).unwrap()));
			}
		} else {
			let is_virtualised = is_x86_feature_detected!("hypervisor");
			if is_virtualised {
				debug!("[TSC] Is virtualized");
				let max_leaf = unsafe { __cpuid(0x40000000) }.eax;
				if max_leaf >= 0x40000010 {
					debug!("[TSC] Leaf 40000010h supported");
					let CpuidResult { eax: freq, .. } = unsafe { __cpuid(0x40000010) };
					if let Some(freq) = NonZero::<u128>::new(freq.into()) {
						debug!("[TSC] Leaf 40000010h enumerated: {freq}kHz");
						multiplier = Some((1000000, freq))
					}
				}
			}
		}

		let intel_msr = || {
			let cpu_brand_name = unsafe { __cpuid(0) };

			if !(cpu_brand_name.ebx == u32::from_le_bytes(*b"Genu")
					&& cpu_brand_name.edx == u32::from_le_bytes(*b"ineI")
					&& cpu_brand_name.ecx == u32::from_le_bytes(*b"ntel")) {
				return None;
			}

			debug!("[TSC] fallback on Intel MSR");

			let (family, model) = {
				let ver_info = unsafe { __cpuid(1) }.eax;
				let family = ver_info.get_bits(8..=11);
				let extended_family = ver_info.get_bits(20..=27);
				let model = ver_info.get_bits(4..=7);
				let extended_model = ver_info.get_bits(16..=19);

				let model = if family == 6 || family == 15 {
					model + (extended_model << 4)
				} else { model };

				let family = if family == 15 { family + extended_family } else { family };

				(family, model)
			};

			debug!("detected (family, model) = ({family:X}h, {model:X}h)");

			let scaler_to_khz = match (family, model) {
				// Nehalem
				(0x06, 0x1A) |
				(0x06, 0x1E) |
				(0x06, 0x1F) |
				(0x06, 0x2E) => 133_330,
				// Sandy bridge
				(0x06, 0x2A) |
				(0x06, 0x2D) |
				// Ivy Bridge
				(0x06, 0x3A) |
				// Ivy Bridge-E
				(0x06, 0x3E) |
				// Xeon Phi
				(0x06, 0x57) |
				(0x06, 0x85) |
				// Haswell
				(0x06, 0x3C) |
				(0x06, 0x45) |
				(0x06, 0x46) => 100_000,
				_ => return None,
			};

			let platform_info = unsafe {
				let (low, high): (u32, u32);
				asm!("rdmsr", in("ecx") 0xCE, out("eax") low, out("edx") high);
				u128::from(low) | u128::from(high) << 32
			};

			let freq_khz = platform_info.get_bits(8..=15) * scaler_to_khz;

			debug!("[TSC] MSR enumerated: {freq_khz}kHz");
			NonZero::<u128>::new(freq_khz).map(|val| (1000000, val))
		};

		let multiplier = multiplier.or_else(intel_msr);

		debug!("multiplier is {multiplier:?}");

		multiplier.unwrap_or_else(|| panic!("Unable to determine TSC frequency"))
	})
}

pub fn local_timer_queue_irq_handler() {
	percpu_v2!(local_timer_queue).handle_irq();
}

pub struct TimerQueue {
	heap: IrqCell<BinaryHeap<Reverse<TimerEvent>>>
}

impl TimerQueue {
	pub const fn new() -> Self {
		Self {
			heap: IrqCell::new(BinaryHeap::new()),
		}
	}
	
	fn fixup_timer(mut heap: IrqGuard<BinaryHeap<Reverse<TimerEvent>>>) {
		let timer = hal::timing::local_timer();
		if let Some(Reverse(next_timer_event)) = heap.peek() {
			// Interrupts disabled here so won't get interrupted as soon as we set the timer
			timer.set_deadline(next_timer_event.time).expect("Could not set timer");
			timer.mask(false);
			
			// By the time we set the timer, system time may have passed the wakeup time without triggering an interrupt
			// Checking the time here will catch it, but there may already be a pending interrupt that gets triggered once we leave
			// this function, so the IRQ handler checks the next event actually is in the past too
			if next_timer_event.time <= Instant::now() {
				let Reverse(event) = heap.pop().expect("Just peeked this so it must exist");
				event.waker.wake(WakeReason::Timeout);
			}
		} else {
			timer.mask(true);
		}
	}
	
	fn append(&self, event: TimerEvent) {
		if event.time <= Instant::now() {
			info!("Waking just pushed timer event ({event:?})");
			event.waker.wake(WakeReason::Timeout);
			return;
		}
		
		let mut guard = self.heap.lock();
		guard.push(
			Reverse(event)
		);

		Self::fixup_timer(guard);
	}
	
	fn handle_irq(&self) {
		let mut guard = self.heap.lock();
		if let Some(Reverse(next_timer_event)) = guard.peek() {
			if next_timer_event.time <= Instant::now() {
				let Reverse(event) = guard.pop().expect("Just peeked this so it must exist");
				event.waker.wake(WakeReason::Timeout);
			}
		}
		Self::fixup_timer(guard);
	}
	
	pub fn waker_for(&self, time: Instant) -> impl Waker + '_ {
		struct W<'a>(&'a TimerQueue, Instant);

		impl<'a> Waker for W<'a> {
			fn add_wake_trigger(&self, waker: WakeTrigger) {
				self.0.append(TimerEvent {
					time: self.1,
					waker,
				})
			}
		}
		
		W(self, time)
	}
}

#[derive(Debug)]
struct TimerEvent {
	time: Instant,
	waker: WakeTrigger,
}

impl PartialEq for TimerEvent {
	fn eq(&self, other: &Self) -> bool {
		self.time.eq(&other.time)
	}
}

impl Eq for TimerEvent {}

impl Ord for TimerEvent {
	fn cmp(&self, other: &Self) -> Ordering {
		self.time.cmp(&other.time)
	}
}

impl PartialOrd for TimerEvent {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}
