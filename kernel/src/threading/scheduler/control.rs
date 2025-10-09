use kernel_api::threading::ThreadId;
use crate::threading::WakeReason;

#[derive(Debug)]
pub enum ControlEvent {
	Unpark(ThreadId, WakeReason),
	Kill(ThreadId),
}
