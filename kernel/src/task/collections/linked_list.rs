use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use kernel_api::ptr::TaggedNonNull;
use kernel_api::threading::TaskRef;
use crate::task::{OwnedTask, TaskRefExt};

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

		let next = {
			let ptr = head.intrusive.next.load(Ordering::Relaxed);
			let ptr = NonNull::new(ptr);
			let task_ref = ptr.map(|ptr| unsafe {
				TaskRef::from_raw(TaggedNonNull::from_tagged_ptr(ptr.cast()))
			});
			task_ref
		};

		head.intrusive.next.store(core::ptr::null_mut(), Ordering::Relaxed);
		self.head = next;
		if next.is_none() {
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
		task.0.intrusive.next.store(core::ptr::null_mut(), Ordering::Relaxed);
		let task_ref = task.0.as_ref();

		let Some(tail_ref) = self.tail.take() else {
			// inserting first task
			self.head = Some(task_ref);
			self.tail = Some(task_ref);
			return;
		};

		let (tail, _) = tail_ref.get_unchecked();
		tail.intrusive.next.store(task_ref.into_raw().as_tagged_ptr().as_ptr().cast(), Ordering::Relaxed);
		self.tail = Some(task_ref);
	}
}

pub enum PopResult {
	None,
	Some(OwnedTask),
	Outdated(OwnedTask),
}
