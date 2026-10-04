use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use kernel_api::ptr::TaggedNonNull;
use kernel_api::threading::TaskRef;
use crate::task::{OwnedTask, Task, TaskRefExt};
use super::PopResult;

#[derive(Debug)]
pub struct DoublyLinkedList {
	head: Option<TaskRef>,
	tail: Option<TaskRef>,
}

impl DoublyLinkedList {
	pub const fn new() -> Self {
		Self {
			head: None,
			tail: None,
		}
	}

	pub fn is_empty(&self) -> bool {
		self.head.is_none()
	}

	pub fn push_back(&mut self, task: OwnedTask) {
		let task_ref = task.0.as_ref();
		let task_ptr = task.0;

		task_ptr.intrusive.next.store(core::ptr::null_mut(), Ordering::Relaxed);

		let Some(tail_ref) = self.tail.take() else {
			// List was empty; new task becomes both head and tail.
			task_ptr.intrusive.prev.store(core::ptr::null_mut(), Ordering::Relaxed);
			self.head = Some(task_ref);
			self.tail = Some(task_ref);
			return;
		};

		let (tail_task, _) = tail_ref.get_unchecked();
		tail_task.intrusive.next.store(task_ref.into_raw().as_tagged_ptr().as_ptr().cast(), Ordering::Relaxed);
		task_ptr.intrusive.prev.store(tail_ref.into_raw().as_tagged_ptr().as_ptr().cast(), Ordering::Relaxed);
		self.tail = Some(task_ref);
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

		if let Some(next) = next {
			next.get_unchecked().0.intrusive.prev.store(core::ptr::null_mut(), Ordering::Relaxed);
			self.head = Some(next);
		} else {
			self.tail = None;
		}

		head.intrusive.next.store(core::ptr::null_mut(), Ordering::Relaxed);
		head.intrusive.prev.store(core::ptr::null_mut(), Ordering::Relaxed);

		if head_actual_generation == head_ref.generation() {
			PopResult::Some(OwnedTask(head))
		} else {
			PopResult::Outdated(OwnedTask(head))
		}
	}

	pub fn remove(&mut self, task: &'static Task) {
		let (next, next_ptr) = {
			let ptr = task.intrusive.next.load(Ordering::Relaxed);
			let ptr_n = NonNull::new(ptr);
			let task_ref = ptr_n.map(|ptr| unsafe {
				TaskRef::from_raw(TaggedNonNull::from_tagged_ptr(ptr.cast()))
			});
			(task_ref, ptr)
		};

		let (prev, prev_ptr) = {
			let ptr = task.intrusive.prev.load(Ordering::Relaxed);
			let ptr_n = NonNull::new(ptr);
			let task_ref = ptr_n.map(|ptr| unsafe {
				TaskRef::from_raw(TaggedNonNull::from_tagged_ptr(ptr.cast()))
			});
			(task_ref, ptr)
		};

		if let Some(prev) = prev {
			prev.get_unchecked().0.intrusive.next.store(next_ptr, Ordering::Relaxed);
		} else {
			// `task` was the head of the list.
			self.head = next;
		}

		if let Some(next) = next {
			next.get_unchecked().0.intrusive.prev.store(prev_ptr, Ordering::Relaxed);
		} else {
			// `task` was the tail of the list.
			self.tail = prev;
		}

		task.intrusive.next.store(core::ptr::null_mut(), Ordering::Relaxed);
		task.intrusive.prev.store(core::ptr::null_mut(), Ordering::Relaxed);
	}
}

impl Default for DoublyLinkedList {
	fn default() -> Self {
		Self::new()
	}
}
