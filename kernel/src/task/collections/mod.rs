mod linked_list;
mod doubly_linked_list;

use core::sync::atomic::Ordering;
pub use linked_list::SinglyLinkedList;
pub use doubly_linked_list::DoublyLinkedList;
use kernel_api::threading::ThreadState;
use crate::task::OwnedTask;

pub enum PopResult {
	None,
	Some(OwnedTask),
	Outdated(OwnedTask),
}

impl PopResult {
	/// Creates a [`PopResult::Outdated`], placing the task into the [`Zombie`](ThreadState::Zombie),
	/// if it was already [`Killed`](ThreadState::Killed).
	pub fn new_outdated(task: OwnedTask) -> Self {
		if let ThreadState::Killed(code) = task.0.state.load(Ordering::Acquire) {
			task.0.state.store(ThreadState::Zombie(code), Ordering::Release);
		}

		PopResult::Outdated(task)
	}
}
