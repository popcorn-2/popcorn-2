use core::time::Duration;
use kernel_api::time::Instant;

fn push_to_global_sleep_queue(_wake_time: Instant) {
	todo!()
}

fn pinned_sleep(time_of_wake: Instant) {
	todo!();
	/* let mut guard = scheduler::SCHEDULER.lock();
	let sleep_event = SchedulerEvent {
		tid: guard.current_thread_id().unwrap(),
		time: time_of_wake,
		action: EventTy::Unblock
	};
	#[cfg(feature = "log.scheduler")] debug!("sleeping tid {:?}", sleep_event.tid);
	guard.event_queue.add(sleep_event);
	guard.block(ThreadState::Sleeping);*/
}

pub fn sleep(duration: Duration) {
	// fixme: if duration <= Duration::from_secs(1) {
	// Core pinned sleep
	pinned_sleep(Instant::now() + duration);
	//} else {
	//	todo!();
	//	push_to_global_sleep_queue(Instant::now() + duration);
	//}
}

pub fn sleep_until(wake_time: Instant) {
	// to avoid having to calculate time until wake, always do a pinned sleep and have it pulled from back of queue later
	pinned_sleep(wake_time);
}
