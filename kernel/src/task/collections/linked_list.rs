use core::sync::atomic::Ordering;
use kernel_api::threading::TaskRef;
use crate::task::{OwnedTask, Task, TaskRefExt};

pub struct SinglyLinkedList {
	head: Option<TaskRef>,
	tail: Option<TaskRef>,
}

impl SinglyLinkedList {
	pub const fn new() -> Self {
		Self {
			head: None,
			tail: None,
		}
	}

	pub fn pop_front(&mut self) -> PopResult {
		let Some(head_ref) = self.head.take() else {
			return PopResult::None;
		};

		let (head, head_actual_generation) = head_ref.get_unchecked();

		let next = unsafe { head.intrusive.next.load(Ordering::Relaxed).cast::<Task>().as_ref() };
		if let Some(next) = next {
			self.head = Some(next.as_ref());
		} else {
			// popped last remaining task
			self.tail = None;
		}

		if head_actual_generation == head_ref.generation() {
			PopResult::Some(OwnedTask(head))
		} else {
			PopResult::Outdated(OwnedTask(head))
		}
	}

	pub fn push_back(&mut self, task: OwnedTask) {
		let Some(tail_ref) = self.tail.take() else {
			// inserting first task
			self.head = Some(task.0.as_ref());
			self.tail = Some(task.0.as_ref());
			return;
		};

		let (tail, _) = tail_ref.get_unchecked();
		tail.intrusive.next.store(task.0.as_ref().as_ptr().as_ptr().cast(), Ordering::Relaxed);
		self.tail = Some(task.0.as_ref());
	}
}

pub enum PopResult {
	None,
	Some(OwnedTask),
	Outdated(OwnedTask),
}
