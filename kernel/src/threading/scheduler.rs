use alloc::collections::{BTreeMap, VecDeque};
use core::borrow::Borrow;
use core::cell::{Cell, UnsafeCell};
use core::fmt::{Debug, Formatter};
use core::mem::{MaybeUninit, swap};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use crate::hal::{HalTy, Hal, ThreadControlBlock, ThreadState};
use core::sync::atomic::{AtomicUsize, Ordering};
use log::debug;

#[thread_local]
pub static SCHEDULER: IrqCell<Scheduler> = IrqCell::new(Scheduler::new());

pub struct IrqCell<T: ?Sized> {
	state: Cell<Option<usize>>,
	data: UnsafeCell<T>
}

impl<T> IrqCell<T> {
	pub const fn new(val: T) -> Self {
		Self { state: Cell::new(None), data: UnsafeCell::new(val) }
	}
}

impl<T: ?Sized> IrqCell<T> {
	pub fn lock(&self) -> IrqGuard<'_, T> {
		// Unsafety: is this actually needed?
		if self.state.get().is_some() { panic!("IrqCell cannot be borrowed multiple times"); }

		self.state.set(Some(HalTy::get_and_disable_interrupts()));
		IrqGuard { cell: self }
	}

	pub unsafe fn unlock(&self) {
		let old_state = self.state.take();
		HalTy::set_interrupts(old_state.unwrap());
	}
}

pub struct IrqGuard<'cell, T: ?Sized> {
	cell: &'cell IrqCell<T>
}

impl<T: Debug> Debug for IrqGuard<'_, T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("IrqGuard")
				.field("cell", &**self)
				.finish()
	}
}

impl<T: ?Sized> Deref for IrqGuard<'_, T> {
	type Target = T;

	fn deref(&self) -> &T {
		unsafe { &*self.cell.data.get() }
	}
}

impl<T: ?Sized> DerefMut for IrqGuard<'_, T> {
	fn deref_mut(&mut self) -> &mut T {
		unsafe { &mut *self.cell.data.get() }
	}
}

impl<T: ?Sized> Drop for IrqGuard<'_, T> {
	fn drop(&mut self) {
		unsafe { self.cell.unlock(); }
	}
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Tid(pub(super) usize);

impl Tid {
	fn new() -> Self {
		static TIDS: AtomicUsize = AtomicUsize::new(1);
		Self(TIDS.fetch_add(1, Ordering::Relaxed))
	}
}

#[derive(Debug)]
pub struct Scheduler {
	pub(super) tasks: BTreeMap<Tid, ThreadControlBlock>,
	pub(super) queue: VecDeque<Tid>,
	pub(super) current_tid: Tid
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct DuplicateKey;

trait BTreeExt<K, V> {
	fn get_many_mut<const N: usize, Q>(&mut self, keys: [&Q; N]) -> Result<[Option<&mut V>; N], DuplicateKey> where K: Borrow<Q> + Ord, Q: Ord + ?Sized;
}

impl<K, V, A: core::alloc::Allocator + Clone> BTreeExt<K, V> for BTreeMap<K, V, A> {
	fn get_many_mut<const N: usize, Q>(&mut self, keys: [&Q; N]) -> Result<[Option<&mut V>; N], DuplicateKey> where K: Borrow<Q> + Ord, Q: Ord + ?Sized {
		fn get_ptr<Q, K, V, A: core::alloc::Allocator + Clone>(this: &mut BTreeMap<K, V, A>, key: &Q) -> Option<NonNull<V>> where K: Borrow<Q> + Ord, Q: Ord + ?Sized {
			this.get_mut(key).map(NonNull::from)
		}

		let mut ptrs = [MaybeUninit::<Option<NonNull<V>>>::uninit(); N];

		for (i, &cur) in keys.iter().enumerate() {
			let ptr = get_ptr(self, cur);

			if ptrs[..i].iter().any(|&prev| unsafe { *prev.assume_init_ref() } == ptr) {
				return Err(DuplicateKey);
			}

			ptrs[i].write(ptr);
		}

		Ok(unsafe { ptrs.transpose().assume_init() }.map(|ptr| ptr.map(|mut ptr| unsafe { ptr.as_mut() })))
	}
}

impl Scheduler {
	pub const fn new() -> Self {
		Self {
			tasks: BTreeMap::new(),
			queue: VecDeque::new(),
			current_tid: Tid(0)
		}
	}

	pub fn add_task(&mut self, tcb: ThreadControlBlock) -> Tid {
		let tid = Tid::new();
		self.tasks.insert(tid, tcb);
		self.queue.push_back(tid);
		tid
	}

	pub fn schedule(&mut self) {
		if let Some(new_tid) = self.queue.pop_front() {
			let old_tid = self.current_tid;
			self.current_tid = new_tid;

			let [old_tcb, new_tcb] = self.tasks.get_many_mut([&old_tid, &new_tid]).expect("Can't switch to same task");
			let old_tcb = old_tcb.expect("Cannot have been running a task that doesn't exist");
			let new_tcb = new_tcb.expect("Next task in queue has already exited");

			if old_tcb.state == ThreadState::Running {
				old_tcb.state = ThreadState::Ready;
				self.queue.push_back(old_tid);
			}

			new_tcb.state = ThreadState::Running;

			unsafe {
				HalTy::switch_thread(old_tcb, new_tcb);
			}
		} else {
			let current_tcb = self.tasks.get(&self.current_tid).expect("Cannot have been running a task that doesn't exist");

			if current_tcb.state == ThreadState::Running { return; }

			panic!("can't yet idle");
		}
	}

	pub fn block(&mut self, state: ThreadState) {
		let current_tcb = self.tasks.get_mut(&self.current_tid).expect("Cannot have been running a task that doesn't exist");
		current_tcb.state = state;
		self.schedule();
	}
}

#[cfg(test)]
mod tests {
	use alloc::collections::BTreeMap;
	use super::*;

	#[test]
	fn btree_dup_key() {
		let mut tree = BTreeMap::from([(1, true), (2, false), (3, true)]);
		assert_eq!(tree.get_many_mut([&1, &1]), Err(DuplicateKey));
	}
}
