use core::mem;
use core::mem::ManuallyDrop;
use kernel_api::allocator::{DynPmm, GlobalAllocator};
use kernel_api::sync::{RwSpinlock, RwUpgradableReadGuard, RwWriteGuard};

#[doc(hidden)]
pub static GLOBAL_HIGHMEM: GlobalAllocator = GlobalAllocator { __rwlock: RwSpinlock::new(None) };
#[doc(hidden)]
pub static GLOBAL_DMA: GlobalAllocator = GlobalAllocator { __rwlock: RwSpinlock::new(None) };

pub fn init_highmem<'a>(allocator: impl Into<DynPmm<'static, true>>) {
	GLOBAL_HIGHMEM.__rwlock.write().replace(allocator.into());
}

pub fn init_dmamem<'a>(allocator: impl Into<DynPmm<'static, true>>) {
	GLOBAL_DMA.__rwlock.write().replace(allocator.into());
}

pub fn with_highmem_as<'a, R, T>(allocator: T, f: impl FnOnce() -> R) -> R where DynPmm<'a, true>: From<T> {
	// FIXME: huge issue in that all allocations get lost therefore only safe to use for bootstrap
	// FIXME(soundness): is this sound?

	let mut write_lock = GLOBAL_HIGHMEM.__rwlock.write();
	let static_highmem = unsafe { mem::transmute::<_, DynPmm<'static, _>>(DynPmm::<'a, true>::from(allocator)) };
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
