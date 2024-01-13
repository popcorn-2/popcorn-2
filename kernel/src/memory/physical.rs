use core::mem;
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicUsize, Ordering};
use kernel_api::memory::allocator::BackingAllocator;
use kernel_api::memory::Frame;
use kernel_api::sync::{RwLock, RwUpgradableReadGuard, RwWriteGuard};

#[export_name = "__popcorn_memory_physical_highmem"]
static GLOBAL_HIGHMEM: RwLock<Option<&'static dyn BackingAllocator>> = RwLock::new(None);
#[export_name = "__popcorn_memory_physical_dmamem"]
static GLOBAL_DMA: RwLock<Option<&'static dyn BackingAllocator>> = RwLock::new(None);

pub use kernel_api::memory::{highmem, dmamem};

pub fn init_highmem<'a>(allocator: &'static dyn BackingAllocator) {
	GLOBAL_HIGHMEM.write().replace(allocator);
}

pub fn init_dmamem<'a>(allocator: &'static dyn BackingAllocator) {
	GLOBAL_DMA.write().replace(allocator);
}

pub fn with_highmem_as<'a, R>(allocator: &'a dyn BackingAllocator, f: impl FnOnce() -> R) -> R {
	// FIXME: huge issue in that all allocations get lost therefore only safe to use for bootstrap
	// FIXME(soundness): is this sound?

	let mut write_lock = GLOBAL_HIGHMEM.write();
	let static_highmem = unsafe { mem::transmute::<_, &'static _>(allocator) };
	let old_highmem = write_lock.replace(static_highmem);

	// To prevent the allocator being changed while the closure is executing, downgrade the write lock to a read lock held across the boundary
	let read_lock = RwWriteGuard::downgrade_to_upgradable(write_lock);

	struct DropGuard<'a, T> {
		lock: ManuallyDrop<RwUpgradableReadGuard<'a, T>>,
		old_val: ManuallyDrop<T>
	}
	impl<T> Drop for DropGuard<'_, T> {
		fn drop(&mut self) {
			let lock = unsafe { ManuallyDrop::take(&mut self.lock) };
			let old_val = unsafe { ManuallyDrop::take(&mut self.old_val) };

			let mut lock = RwUpgradableReadGuard::upgrade(lock);
			*lock = old_val;
		}
	}

	let _drop_guard = DropGuard {
		lock: ManuallyDrop::new(read_lock),
		old_val: ManuallyDrop::new(old_highmem)
	};

	f()
}

static REFCOUNTS: [RefCountEntry; 0] = [];

struct RefCountEntry {
	strong_count: AtomicUsize,
	next_segment: Option<AtomicUsize>
}

impl RefCountEntry {
	fn increment(&self) {
		self.strong_count.fetch_add(1, Ordering::Relaxed);
	}

	fn decrement(&self) -> bool {
todo!()
	}
}

/// Conceptually the same as a hypothetical `Arc<[Frame]>`
///
/// # Invariants
///
/// Internal implementation assumes that two `ArcFrames` cannot overlap unless they came from the same original `ArcFrames`
/// object
pub struct ArcFrames {
	base: Frame,
	len: usize
}

impl ArcFrames {
	unsafe fn new(base: Frame, len: usize) -> Self {
		Self {
			base, len
		}
	}

	fn split_at(self, n: usize) -> (Self, Self) {
		assert!(n <= self.len);

		let second_base = self.base + n;
		let lens = (n, self.len - n);

		todo!("Insert new RC node");

		(Self { base: self.base, len: lens.0 }, Self { base: second_base, len: lens.1 })
	}
}

impl Clone for ArcFrames {
	fn clone(&self) -> Self {
		todo!("Update RC nodes");
		Self { .. *self }
	}
}

impl Drop for ArcFrames {
	fn drop(&mut self) {
		todo!()
	}
}
