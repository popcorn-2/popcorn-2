use crate::threading::{ThreadId, WakeReason};

#[derive(Debug)]
pub enum ControlEvent {
	Unpark(ThreadId, WakeReason),
}
