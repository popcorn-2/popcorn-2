use alloc::sync::Arc;
use core::fmt::{Debug, Formatter};
use core::ops::Range;
#[allow(unused_imports)] use crate::prelude::*;
use core::ptr::{addr_of, NonNull};
use core::sync::atomic::{AtomicPtr, Ordering};
use kernel_api::memory::{AllocError, Page, VirtualAddress};
use kernel_api::memory::mapping::{DynMapping, Mappable, Mapping, RawMapping};
use kernel_api::memory::physical::highmem;
use kernel_api::memory::r#virtual::{address_space::AddressSpace, Userspace, VirtualAllocator};
use kernel_api::sync::{RwSpinlock, Spinlock};
use ranged_btree_allocator::RangedBtreeAllocator;

#[export_name = "__popcorn_memory_virtual_kernel_global"]
pub static GLOBAL_VIRTUAL_ALLOCATOR: RwSpinlock<&'static dyn VirtualAllocator> = RwSpinlock::new(&BOOTSTRAP);

extern "C" {
	static __popcorn_vmem_bootstrap_start: u8;
	static __popcorn_vmem_bootstrap_end: u8;
}

pub struct SyncWrapper(pub *mut u8);
unsafe impl Sync for SyncWrapper {}

pub static VMEM_BOOTSTRAP_START: SyncWrapper = SyncWrapper(unsafe { addr_of!(__popcorn_vmem_bootstrap_start) as *mut _ });
pub static VMEM_BOOTSTRAP_END: SyncWrapper = SyncWrapper(unsafe { addr_of!(__popcorn_vmem_bootstrap_end) as *mut _ });

static BOOTSTRAP: Bootstrap = Bootstrap {
	start: AtomicPtr::new(VMEM_BOOTSTRAP_START.0)
};

pub struct Bootstrap {
	start: AtomicPtr<u8>
}

impl VirtualAllocator for Bootstrap {
	fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
		let old = match len {
			0 => 0 as _,
			1.. => self.start.fetch_byte_add(len * 4096, Ordering::Relaxed)
		};

		if (old as usize) + (len * 4096) > VMEM_BOOTSTRAP_END.0 as usize { return Err(AllocError); }

		Ok(Page::new(VirtualAddress::new(old as usize)))
	}

	fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
		let current_end = self.start.load(Ordering::Relaxed);

		debug!("Bootstrap VMA current_end={current_end:#p}");

		if current_end != at.start().as_ptr() {
			debug!("`at` end doesn't match");
			return Err(AllocError);
		}
		
		let new_end = unsafe { current_end.byte_add(len * 4096) };
		if new_end >= VMEM_BOOTSTRAP_END.0 {
			debug!("exhausted vmem_bootstrap region");
			return Err(AllocError);
		}

		// This should never be contested since bootstrap only runs on one core, so fine to not do a cmpxchg loop
		match self.start.compare_exchange(current_end, new_end, Ordering::Relaxed, Ordering::Relaxed) {
			Ok(_) => Ok(at),
			Err(_) => Err(AllocError)
		}
	}

	fn deallocate_contiguous(&self, _: Page, _: usize) {}
}

#[allow(unused_imports)]
use crate::hal::paging2::TTable;
use crate::hal::TTableTy;
use crate::memory::paging::ktable;

pub(crate) struct AddressSpaceInner {
	ttable: TTableTy,
	allocator: RangedBtreeAllocator,
	maps: Spinlock<Vec<(&'static str, RawMapping<'static, DynMapping, Userspace>)>>,
}

impl Debug for AddressSpaceInner {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		writeln!(f, "AddressSpaceInner {{")?;
		for (name, map) in &*self.maps.lock() {
			writeln!(f, "    {:x}-{:x} {} {:?}", map.virtual_start().as_ptr().addr(), map.virtual_end().as_ptr().addr(), name, map.protection())?;
		}
		writeln!(f, "}}")?;
		Ok(())
	}
}

// Alignment of the extern type version must be known, so ensure it's the same value here
const _: () = { assert!(align_of::<AddressSpaceInner>() == 8); };

impl AddressSpaceInner {
	pub fn new(ttable: TTableTy, allocator: RangedBtreeAllocator) -> Arc<AddressSpaceInner> {
		Arc::new(Self { ttable, allocator, maps: Spinlock::new(vec![]) })
	}
	
	pub fn empty() -> Result<Arc<AddressSpaceInner>, AllocError> {
		Ok(Self::new(
			TTableTy::new(&*ktable(), highmem())?,
			RangedBtreeAllocator::new(Range {
				start: Page::new(VirtualAddress::new(0x200000)),
				end: Page::new(VirtualAddress::new(0x8000_0000_0000)),
			}),
		))
	}

	pub fn ttable(&self) -> &TTableTy { &self.ttable }

	pub fn to_api(this: &Arc<Self>) -> &AddressSpace {
		// can't use cast here because it requires `U: Sized` in case of metadata,
		// but we have `U: !Sized` but `U: Thin` so there isn't any metadata
		// additionally, `Arc::from_raw` calculates the offset back to the counts
		// internally calling align_of, which isn't valid on extern types
		// SAFETY: `kernel_api::AddressSpaceInner` is an extern type so it can refer to any other type
		unsafe { core::mem::transmute::<_, &AddressSpace>(this) }
	}

	pub fn from_api(this: &AddressSpace) -> &Arc<Self> {
		unsafe { core::mem::transmute::<_, &Arc<Self>>(this) }
	}
	
	pub fn add_mapping<M: Mappable>(&self, name: &'static str, map: RawMapping<'static, M, Userspace>) where [(); 1 / ((size_of::<M>() == 0) as usize)]: {
		self.maps.lock().push((name, DynMapping::coerce(map)));
	}
}

#[no_mangle]
unsafe fn __popcorn_address_space_load(this: &AddressSpaceInner) {
	unsafe { this.ttable.load(); }
}

#[no_mangle]
fn __popcorn_check_address_space(to_check: NonNull<AddressSpaceInner>) -> bool {
	let guard = percpu_v2!(current_thread).read();
	let address_space = guard
			.as_ref().expect("cannot use User<*> from idle thread")
			.tcb_ref().address_space;
	core::ptr::eq(Arc::as_ptr(address_space), to_check.as_ptr().cast_const())
}

#[no_mangle]
fn __popcorn_address_space_allocate(this: &AddressSpaceInner, len: usize) -> Result<Page, AllocError> {
	this.allocator.allocate_contiguous(len)
}

#[no_mangle]
fn __popcorn_address_space_allocate_at(this: &AddressSpaceInner, at: Page, len: usize) -> Result<Page, AllocError> {
	this.allocator.allocate_contiguous_at(at, len)
}

#[no_mangle]
fn __popcorn_address_space_deallocate(this: &AddressSpaceInner, base: Page, len: usize) {
	this.allocator.deallocate_contiguous(base, len)
}
