use core::time::Duration;
use kernel_api::time::Instant;
use crate::threading::WakeReason;

pub fn sleep(duration: Duration) {
	sleep_until(Instant::now() + duration);
}

pub fn sleep_until(wake_time: Instant) {
	let reason = super::park(&[todo!()]).expect("Failed to park");
	debug_assert_eq!(reason, WakeReason::Timeout, "`sleep` should only wake due to a timeout");
}
