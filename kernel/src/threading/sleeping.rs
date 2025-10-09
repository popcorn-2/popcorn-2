use core::time::Duration;
use kernel_api::time::Instant;
use crate::prelude::percpu_v2;

#[expect(unused)]
pub async fn sleep(duration: Duration) {
	sleep_until(Instant::now() + duration).await;
}

pub async fn sleep_until(wake_time: Instant) {
	percpu_v2!(local_timer_queue).wait_until(wake_time).await;
}
