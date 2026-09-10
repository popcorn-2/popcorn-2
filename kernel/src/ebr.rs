use core::ptr;
use core::ops::Deref;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use kernel_api::sync::Spinlock;

static GLOBAL_EPOCH: AtomicUsize = AtomicUsize::new(0);
static LOCAL_EPOCHS: AtomicPtr<LocalEpoch> = AtomicPtr::new(ptr::null_mut());
const INACTIVE_BIT: usize = 1 << (usize::BITS - 1);

pub struct EpochGuard<'e> {
	local_epoch: &'e LocalEpoch,
	inner: kernel_api::memory::EpochGuard,
}

impl Drop for EpochGuard<'_> {
	fn drop(&mut self) {
		self.local_epoch.epoch.fetch_or(INACTIVE_BIT, Ordering::Release);
	}
}

pub struct LocalEpoch {
	next: AtomicPtr<LocalEpoch>,
	epoch: AtomicUsize,
	retired: Spinlock<Vec<Retired>>,
}

impl LocalEpoch {
	pub const fn new() -> Self {
		Self {
			next: AtomicPtr::new(ptr::null_mut()),
			epoch: AtomicUsize::new(INACTIVE_BIT),
			retired: Spinlock::new(Vec::new()),
		}
	}

	pub fn pin(&self) -> EpochGuard<'_> {
		let global_epoch = GLOBAL_EPOCH.load(Ordering::Acquire) & !INACTIVE_BIT;
		self.epoch.store(global_epoch, Ordering::Release);
		EpochGuard {
			local_epoch: self,
			inner: unsafe { kernel_api::memory::EpochGuard::new() }
		}
	}
}

pub fn defer_and_cleanup(drop_fn: fn(*mut u8), val: *mut u8) {
	let mut guard = percpu_v2!(epoch).retired.lock();
	if guard.len() == guard.capacity() {
		gc_collect();
	}
	guard.push(Retired {
		in_epoch: GLOBAL_EPOCH.load(Ordering::Acquire) & !INACTIVE_BIT,
		drop_fn,
		val,
	});
}

impl Deref for EpochGuard<'_> {
	type Target = kernel_api::memory::EpochGuard;

	fn deref(&self) -> &Self::Target { &self.inner }
}

const _: () = {
	const fn assert_send_sync<T: Send + Sync>() {}
	assert_send_sync::<LocalEpoch>();
};

pub fn init() {
	let local_epoch = percpu_v2!(epoch);
	*local_epoch.retired.lock() = Vec::with_capacity(64);
	let mut global_next = LOCAL_EPOCHS.load(Ordering::Acquire);
	loop {
		local_epoch.next.store(global_next, Ordering::Relaxed);
		match LOCAL_EPOCHS.compare_exchange_weak(
			global_next,
			ptr::from_ref(local_epoch).cast_mut(),
			Ordering::Release,
			Ordering::Acquire,
		) {
			Ok(_) => break,
			Err(old) => global_next = old,
		}
	}
}

struct Retired {
	in_epoch: usize,
	drop_fn: fn(*mut u8),
	val: *mut u8,
}

unsafe impl Send for Retired {}
unsafe impl Sync for Retired {}

impl Drop for Retired {
	fn drop(&mut self) {
		(self.drop_fn)(self.val);
	}
}

struct LocalEpochIterator {
	current: Option<&'static LocalEpoch>,
}

impl LocalEpochIterator {
	fn new() -> Self {
		let current = LOCAL_EPOCHS.load(Ordering::Acquire);
		// SAFETY: `next` pointer is equivalent to Atomic<Option<&'static LocalEpoch>>
		let current = unsafe { current.as_ref() };
		Self {
			current,
		}
	}
}

impl Iterator for LocalEpochIterator {
	type Item = &'static LocalEpoch;

	fn next(&mut self) -> Option<Self::Item> {
		let current = self.current?;
		let next = current.next.load(Ordering::Acquire);
		// SAFETY: `next` pointer is equivalent to Atomic<Option<&'static LocalEpoch>>
		self.current = unsafe { next.as_ref() };
		Some(current)
	}
}

pub fn gc_collect() {
	debug!("running garbage collection");

	let global_epoch = GLOBAL_EPOCH.load(Ordering::Acquire);

	let all_cpus_current = LocalEpochIterator::new().all(|local_epoch| {
		let state = local_epoch.epoch.load(Ordering::Acquire);
		let is_inactive = (state & INACTIVE_BIT) != 0;
		let cpu_epoch = state & !INACTIVE_BIT;

		is_inactive || cpu_epoch == (global_epoch & !INACTIVE_BIT)
	});

	if all_cpus_current {
		let global_epoch = GLOBAL_EPOCH.compare_exchange(
			global_epoch,
			global_epoch.wrapping_add(1),
			Ordering::Release,
			Ordering::Acquire,
		).unwrap_or_else(|val| val);

		let mut retire_queue = percpu_v2!(epoch).retired.lock();
		retire_queue.retain(|retired| retired.in_epoch >= (global_epoch & !INACTIVE_BIT));
	}
}
