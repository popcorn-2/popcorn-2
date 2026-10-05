use alloc::boxed::Box;
use core::hash::Hash;
use core::ops::ControlFlow;
use core::pin::Pin;
use hashbrown::hash_map::DefaultHashBuilder;
use hashbrown::HashMap;
use kernel_api::memory::VirtualAddress;
use kernel_api::num::ufat;
use kernel_api::ptr::LocalUser;
use kernel_api::sync::Spinlock;
use kernel_api::{address_space, syscall};
use crate::task::{Task, WaitQueue};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Key {
	address_space: address_space::Key,
	address: VirtualAddress,
}

static FUTEX_TABLE: Spinlock<HashMap<Key, Pin<Box<WaitQueue>>>> = Spinlock::new(HashMap::with_hasher(DefaultHashBuilder::new()));

/// Puts the calling task to sleep on the futex at `address` if `*address == expected`.
///
/// # Errors
/// - [`syscall::Error::InvalidArg`] if `address` is not valid for a futex variable.
/// - [`syscall::Error::InvalidPointer`] if `address` could not be accessed.
/// - [`syscall::Error::Again`] if `*address != expected`.
pub fn futex_wait(caller: &Task, address: VirtualAddress, expected: u32) -> syscall::Result<ufat> {
	if !address.is_aligned_to(4) || address.is_higher_half() {
		return Err(syscall::Error::InvalidArg);
	}

	let key = Key {
		address_space: caller.address_space.hashable_key(),
		address,
	};

	let mut guard = FUTEX_TABLE.lock();
	let queue = guard.entry(key)
		.or_insert_with(|| Box::pin(WaitQueue::new()));

	// Enqueue the current task on the wait queue using `wait_with`.
	// The bucket lock is dropped inside the `before_reschedule` callback
	// after the task has been safely linked into the wait queue.
	if let Some(result) = queue.as_ref().wait_if(|| {
		// SAFETY: `uaddr` is 4-byte aligned and checked to be a userspace address.
		let ptr = unsafe { LocalUser::<*const u32>::new(address.addr) };
		let current_val = ptr.read();

		match current_val {
			Ok(current_val) if current_val == expected => { ControlFlow::Continue(()) },
			Ok(_) => ControlFlow::Break(syscall::Error::ConditionNotMet),
			Err(err) => ControlFlow::Break(err.into()),
		}
	}).break_value() {
		Err(result)
	} else {
		Ok(ufat::new(0, 0))
	}
}

/// Wakes up at most `count` tasks waiting on the futex at `address`.
///
/// # Errors
/// - [`syscall::Error::InvalidArg`] if `address` is not valid for a futex variable.
///
/// Returns the number of tasks actually woken.
pub fn futex_wake(caller: &Task, address: VirtualAddress, count: usize) -> syscall::Result<ufat> {
	if !address.is_aligned_to(4) || address.is_higher_half() {
		return Err(syscall::Error::InvalidArg);
	}

	let key = Key {
		address_space: caller.address_space.hashable_key(),
		address,
	};

	let mut guard = FUTEX_TABLE.lock();

	let Some(queue) = guard.get(&key) else {
		return Ok(ufat::new(0, 0));
	};

	let woken = queue.as_ref().wake_n(count);

	if queue.is_empty() {
		guard.remove(&key);
	}

	Ok(ufat::new(0, woken))
}
