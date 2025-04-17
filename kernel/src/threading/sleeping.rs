use core::time::Duration;
use kernel_api::time::Instant;
use crate::prelude::percpu_v2;
use crate::threading::WakeReason;

pub fn sleep(duration: Duration) {
	sleep_until(Instant::now() + duration);
}

pub fn sleep_until(wake_time: Instant) {
	let reason = super::park(&[&percpu_v2!(local_timer_queue).waker_for(wake_time)]).expect("Failed to park");
	debug_assert_eq!(reason, WakeReason::Timeout, "`sleep` should only wake due to a timeout");
}
