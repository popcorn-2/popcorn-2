#![cfg_attr(not(loom), no_std)]

extern crate alloc;

use alloc::boxed::Box;
#[cfg(not(loom))] use core::sync::atomic::AtomicPtr;
#[cfg(loom)] use loom::sync::atomic::AtomicPtr;
use core::sync::atomic::Ordering;

pub struct LockFreeRadixTreeU32L4<V>(Block<Block<Block<Block<V, 256>, 256>, 256>, 256>);

struct Block<V, const SIZE: usize> {
	entries: [AtomicPtr<V>; SIZE],
}

impl<V, const SIZE: usize> Default for Block<V, SIZE> {
	#[cfg(not(loom))]
	fn default() -> Self {
		Self {
			entries: [const { AtomicPtr::new(core::ptr::null_mut()) }; _],
		}
	}

	#[cfg(loom)]
	fn default() -> Self {
		Self {
			entries: core::array::from_fn(|_| AtomicPtr::new(core::ptr::null_mut()))
		}
	}
}

impl<V, const SIZE: usize, const SUB_SIZE: usize> Block<Block<V, SUB_SIZE>, SIZE> {
	fn get_or_add_sublock_at(&self, idx: usize) -> &Block<V, SUB_SIZE> {
		let block = self.entries[idx].load(Ordering::Acquire);
		let block = if block.is_null() {
			let block = Box::<Block<V, SUB_SIZE>>::default();
			let block = Box::into_raw(block);

			let res = self.entries[idx].compare_exchange(
				core::ptr::null_mut(),
				block,
				Ordering::AcqRel,
				Ordering::Acquire,
			);
			if res.is_err() { unsafe { drop(Box::from_raw(block)); } }
			res.map(|_| block).unwrap_or_else(|block| block)
		} else { block };

		unsafe { block.as_ref().unwrap() }
	}
}

impl<V> LockFreeRadixTreeU32L4<V> {
	#[cfg(not(loom))]
	pub const fn new() -> Self {
		Self(Block {
			entries: [const { AtomicPtr::new(core::ptr::null_mut()) }; _],
		})
	}

	#[cfg(loom)]
	pub fn new() -> Self {
		Self(Block {
			entries: core::array::from_fn(|_| AtomicPtr::new(core::ptr::null_mut()))
		})
	}

	pub fn get(&self, key: u32) -> Option<&AtomicPtr<V>> {
		let l1 = (key >> 24) as usize;
		let l2 = ((key >> 16) & 0xff) as usize;
		let l3 = ((key >> 8) & 0xff) as usize;
		let l4 = (key & 0xff) as usize;

		let ptr = self.0.entries[l1].load(Ordering::Acquire);
		let block = unsafe { ptr.as_ref()? };
		let ptr = block.entries[l2].load(Ordering::Acquire);
		let block = unsafe { ptr.as_ref()? };
		let ptr = block.entries[l3].load(Ordering::Acquire);
		let block = unsafe { ptr.as_ref()? };
		Some(&block.entries[l4])
	}

	pub fn insert(&self, key: u32, val: *mut V) -> Result<(), *mut V> {
		let l1 = (key >> 24) as usize;
		let l2 = ((key >> 16) & 0xff) as usize;
		let l3 = ((key >> 8) & 0xff) as usize;
		let l4 = (key & 0xff) as usize;

		let block = self.0.get_or_add_sublock_at(l1);
		let block = block.get_or_add_sublock_at(l2);
		let block = block.get_or_add_sublock_at(l3);
		block.entries[l4].compare_exchange(
			core::ptr::null_mut(),
			val,
			Ordering::Release,
			Ordering::Acquire,
		).map(|_| ())
	}
}

#[cfg(test)]
mod tests {
	#[cfg(loom)] use std::sync::Arc;
	use super::*;

	#[cfg(not(loom))]
	#[test]
	fn insert_and_get() {
		let mut x = 5;

		let tree = LockFreeRadixTreeU32L4::new();
		tree.insert(0, &raw mut x).unwrap();
		assert_eq!(tree.get(0).unwrap().load(Ordering::Acquire), &raw mut x);
		assert!(tree.get(555).is_none());
	}

	#[cfg(loom)]
	#[test]
	fn insert_and_get_threaded() {
		loom::model(|| {
			let x = Box::into_raw(Box::new(5));
			let y = Box::into_raw(Box::new(6));
			let tree = Arc::new(LockFreeRadixTreeU32L4::new());
			let tree2 = Arc::clone(&tree);
			loom::thread::spawn(move || {
				if let Err(ptr) = tree2.insert(0, x) {
					assert_eq!(ptr, y)
				}
			});
			if let Err(ptr) = tree.insert(0, y) {
				assert_eq!(ptr, x)
			}
			let val = tree.get(0).unwrap().load(Ordering::Acquire);
			assert!(val == x || val == y);
			assert!(tree.get(555).is_none());
		});
	}

	#[test]
	fn insert_distinct_shared_prefix() {
		loom::model(|| {
			let tree = Arc::new(LockFreeRadixTreeU32L4::new());
			let tree2 = Arc::clone(&tree);

			let x = Box::into_raw(Box::new(1));
			let y = Box::into_raw(Box::new(2));

			let handle = loom::thread::spawn(move || {
				assert!(tree2.insert(1, x).is_ok());
			});

			assert!(tree.insert(2, y).is_ok());

			handle.join().unwrap();

			// Both must exist, proving no branch was overwritten
			assert_eq!(tree.get(1).unwrap().load(Ordering::Acquire), x);
			assert_eq!(tree.get(2).unwrap().load(Ordering::Acquire), y);
		});
	}

	#[cfg(loom)]
	#[test]
	fn read_while_write() {
		loom::model(|| {
			let tree = Arc::new(LockFreeRadixTreeU32L4::new());
			let tree2 = Arc::clone(&tree);
			let val = Box::into_raw(Box::new(42));

			loom::thread::spawn(move || {
				tree2.insert(0xFF, val).unwrap();
			});

			// Spin until the value appears, then verify it is completely valid
			// and no segfault/panic occurs during traversal.
			loop {
				if let Some(atomic_ptr) = tree.get(0xFF) {
					let ptr = atomic_ptr.load(Ordering::Acquire);
					if !ptr.is_null() {
						unsafe { assert_eq!(*ptr, 42); }
						break;
					}
				}
				loom::thread::yield_now();
			}
		});
	}

	#[cfg(loom)]
	#[test]
	fn count_errors() {
		loom::model(|| {
			let w = Box::into_raw(Box::new(4));
			let x = Box::into_raw(Box::new(5));
			let z = Box::into_raw(Box::new(7));
			let tree = Arc::new(LockFreeRadixTreeU32L4::new());
			let tree2 = Arc::clone(&tree);
			let tree3 = Arc::clone(&tree);

			// insert a dummy node with higher levels shared to reduce the number of CAS
			// loops and preventing the time to run blowing up
			tree.insert(1 << 10, core::ptr::null_mut()).unwrap();

			let t1 = loom::thread::spawn(move || tree2.insert(0, w).is_ok());
			let t2 = loom::thread::spawn(move || tree3.insert(0, x).is_ok());

			let main_success = tree.insert(0, z).is_ok();

			let t1_success = t1.join().unwrap();
			let t2_success = t2.join().unwrap();

			let total_successes = (t1_success as usize) + (t2_success as usize) + (main_success as usize);

			assert_eq!(total_successes, 1, "Exactly one thread should succeed");
		});
	}
}
