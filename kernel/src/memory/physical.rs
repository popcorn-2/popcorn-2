use alloc::sync::Arc;
use core::mem;
use core::ops::Deref;
use core::sync::atomic::{AtomicUsize, Ordering};
use kernel_api::memory::allocator::{BackingAllocator, GlobalAllocator};
use kernel_api::memory::Frame;
use kernel_api::sync::Mutex;
use kernel_api::sync::{RwLock, RwReadGuard, RwUpgradableReadGuard, RwWriteGuard};

static GLOBAL_HIGHMEM: RwLock<Option<GlobalAllocator>> = RwLock::new(None);
static GLOBAL_DMA: RwLock<Option<GlobalAllocator>> = RwLock::new(None);

#[inline]
pub fn highmem() -> impl Deref<Target = GlobalAllocator> {
	RwReadGuard::map(
		GLOBAL_HIGHMEM.read(),
		|inner| inner.as_ref().expect("No highmem allocator")
	)
}

pub fn set_highmem(allocator: Arc<dyn BackingAllocator>) {
	let mut write_lock = GLOBAL_HIGHMEM.write();
	write_lock.replace(GlobalAllocator::Arc(allocator));
}

pub fn with_highmem_as<'a, R>(allocator: &'a dyn BackingAllocator, f: impl FnOnce() -> R) -> R {
	// FIXME: huge issue in that all allocations get lost therefore only safe to use for bootstrap
	// FIXME(soundness): is this sound?

	let mut write_lock = GLOBAL_HIGHMEM.write();
	let static_highmem = unsafe { mem::transmute::<_, &'static _>(allocator) };
	let old_highmem = write_lock.replace(GlobalAllocator::Static(static_highmem));

	// To prevent the allocator being changed while the closure is executing, downgrade the write lock to a read lock held across the boundary
	let read_lock = RwWriteGuard::downgrade_to_upgradable(write_lock);

	let ret = crate::panicking::catch_unwind(f);

	let mut write_lock = RwUpgradableReadGuard::upgrade(read_lock);
	*write_lock = old_highmem;

	match ret {
		Ok(ret) => ret,
		Err(payload) => crate::panicking::resume_unwind(payload)
	}
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
