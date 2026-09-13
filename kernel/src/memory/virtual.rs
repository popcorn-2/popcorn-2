use alloc::borrow::Cow;
use alloc::sync::Arc;
use core::fmt::{Debug, Formatter};
use core::ops::{Deref, DerefMut, Range};
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicPtr, Ordering};
use slab::Slab;
use kernel_api::address_space::{AddressSpace, MappingKey, Userspace};
use kernel_api::allocator::{AllocError, highmem, Vmm};
use kernel_api::mapping::{Mappable, Mapping, Ty};
use kernel_api::memory::{PAGE_SIZE, RawPage, VirtualAddress, RawFrame, PhysicalAddress};
use kernel_api::sync::{LazyLock, MappedSpinlockGuard, RwSpinlock, Spinlock, SpinlockGuard};
use linked_list_allocator::LinkedListAllocator;

pub static GLOBAL_VIRTUAL_ALLOCATOR: LazyLock<RwSpinlock<&'static (dyn Vmm + Sync)>> = LazyLock::new(|| RwSpinlock::new(BOOTSTRAP.deref()));

unsafe extern "C" {
	static mut __popcorn_vmem_bootstrap_end: u8;
}

pub fn vmem_bootstrap_end() -> *mut u8 {
	addr_of_mut!(__popcorn_vmem_bootstrap_end)
}

pub fn vmem_bootstrap_start() -> *mut u8 {
	unsafe { vmem_bootstrap_end().byte_sub(8*1024*1024) }
}

static BOOTSTRAP: LazyLock<Bootstrap> = LazyLock::new(|| {
	Bootstrap {
		start: AtomicPtr::new(vmem_bootstrap_start().cast())
	}
});

pub struct Bootstrap {
	start: AtomicPtr<u8>
}

impl Vmm for Bootstrap {
	fn allocate_contiguous(&self, len: usize) -> Result<RawPage, AllocError> {
		let old = match len {
			0 => 0 as _,
			1.. => self.start.fetch_byte_add(len * 4096, Ordering::Relaxed)
		};

		if (old as usize) + (len * 4096) > vmem_bootstrap_end().addr() { return Err(AllocError::vmm()); }

		debug_assert!(VirtualAddress::from(old).is_aligned_to(PAGE_SIZE));
		Ok(VirtualAddress::from(old).align_down_to_page())
	}

	fn allocate_contiguous_at(&self, at: RawPage, len: usize) -> Result<RawPage, AllocError> {
		let current_end = self.start.load(Ordering::Relaxed);

		debug!("Bootstrap VMA current_end={current_end:#p}");

		if current_end != at.as_ptr() {
			debug!("`at` end doesn't match");
			return Err(AllocError::vmm());
		}
		
		let new_end = unsafe { current_end.byte_add(len * 4096) };
		if new_end >= vmem_bootstrap_end() {
			debug!("exhausted vmem_bootstrap region");
			return Err(AllocError::vmm());
		}

		// This should never be contested since bootstrap only runs on one core, so fine to not do a cmpxchg loop
		match self.start.compare_exchange(current_end, new_end, Ordering::Relaxed, Ordering::Relaxed) {
			Ok(_) => Ok(at),
			Err(_) => Err(AllocError::vmm())
		}
	}

	fn deallocate_contiguous(&self, _: RawPage, _: usize) {}
}

use crate::hal::paging2::{Flags, KTable, TTable};
use crate::hal::paging::MapPageError;
use crate::hal::TTableTy;
use crate::memory::paging::ktable;

pub(crate) struct AddressSpaceInner {
	ttable: TTableTy,
	allocator: LinkedListAllocator,
	maps: Spinlock<Slab<(Cow<'static, str>, Mapping<Box<dyn Mappable + Send>, Userspace>)>>,
}

const _: () = {
	// This is required to justify the Send + Sync impl on the opaque version of AddressSpaceInner in kernel_api
	const fn check_address_space_inner<T: Send + Sync + ?Sized>() {}
	check_address_space_inner::<AddressSpaceInner>();
};

impl Debug for AddressSpaceInner {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		writeln!(f, "AddressSpaceInner {{")?;
		for (_, (name, map)) in &*self.maps.lock() {
			writeln!(
				f,
				"    {:#p}-{:#p} {} {:?}",
				map.as_ptr_range().start,
				map.as_ptr_range().end,
				name,
				"---",
			)?;
		}
		writeln!(f, "}}")?;
		Ok(())
	}
}

mod private {
	pub trait Sealed {}
}

pub trait AddressSpaceExt: private::Sealed + Sized {
	fn from_parts(ttable: TTableTy, allocator: LinkedListAllocator) -> Self;
	fn empty() -> Result<Self, AllocError>;
	fn get(&self, key: MappingKey) -> Option<MappingEntry<'_>>;
	fn ttable(&self) -> &TTableTy;
	unsafe fn load(&self);
}

impl private::Sealed for AddressSpace {}

fn to_inner(address_space: &AddressSpace) -> &AddressSpaceInner {
	let inner = unsafe { &*address_space.__extract_ptr(Ordering::SeqCst) };
	inner.downcast_ref().expect("`AddressSpace` should contain an `AddressSpaceInner`")
}

impl AddressSpaceExt for AddressSpace {
	fn from_parts(ttable: TTableTy, allocator: LinkedListAllocator) -> Self {
		let inner = AddressSpaceInner {
			ttable,
			allocator,
			maps: Spinlock::new(Slab::new())
		};
		Self::__new(Arc::new(inner))
	}

	fn empty() -> Result<Self, AllocError> {
		Ok(AddressSpaceExt::from_parts(
			TTableTy::new(&*ktable(), highmem())?,
			LinkedListAllocator::new(Range {
				start: RawPage::new(0x200000),
				end: RawPage::new(0x8000_0000_0000),
			})?,
		))
	}

	fn get(&self, key: MappingKey) -> Option<MappingEntry<'_>> {
		let _guard = self.__assert_in_use();
		let inner = to_inner(self);
		let mut guard = inner.maps.lock();
		let entry = guard.get_mut(key.0)?;
		Some(MappingEntry {
			key,
			data: addr_of_mut!(entry.1),
			spinlock: guard,
		})
	}

	/// Loads the page tables associated with this [`AddressSpace`]
	/// 
	/// # Safety
	/// 
	/// - The [`AddressSpace`] must not be dropped or swapped until another call to `load`
	/// - All types held across the call to `load` must be [`Send`] and [`Sync`]
	unsafe fn load(&self) {
		let inner = to_inner(self);
		unsafe { inner.ttable.load() };
	}

	fn ttable(&self) -> &TTableTy {
		let inner = to_inner(self);
		&inner.ttable
	}
}

#[unsafe(no_mangle)]
fn __popcorn_address_space_push_mapping<'a>(this: &'a AddressSpace, name: Cow<'static, str>, mapping: Mapping<Box<dyn Mappable + Send>, Userspace>) -> (MappingKey, MappedSpinlockGuard<'a, Mapping<Box<dyn Mappable + Send>, Userspace>>) {
	let _guard = this.__assert_in_use();
	let inner = to_inner(this);
	let mut guard = inner.maps.lock();
	let key = guard.insert((name, mapping));
	(
		MappingKey(key),
		SpinlockGuard::map(guard, |slab| &mut slab.get_mut(key).expect("just inserted this").1)
	)
}

#[unsafe(no_mangle)]
fn __popcorn_address_space_get_allocator(this: &AddressSpace) -> &'_ (dyn Vmm + 'static) {
	let _guard = this.__assert_in_use();
	let inner = to_inner(this);
	&inner.allocator
}

#[unsafe(no_mangle)]
fn __popcorn_upt_map_contiguous(this: &AddressSpace, base_page: RawPage, base_frame: RawFrame, count: usize, ty: Ty, flags: Flags) -> Result<(), MapPageError> {
	debug!("upt map {ty:?} {flags:?}");
	let _guard = this.__assert_in_use();
	let inner = to_inner(this);

	let mut remaining = count;
	while remaining > 0 {
		match inner.ttable.map_page(
			base_page + (count - remaining),
			base_frame + (count - remaining),
			ty,
			flags,
		) {
			Ok(_) => {},
			Err(e) => {
				for page in base_page .. (base_page + (count - remaining)) {
					let res = inner.ttable.unmap_page(page);
					debug_assert!(res.is_ok(), "just mapped this page");
				}
				return Err(e);
			}
		}
		remaining -= 1;
	}

	Ok(())
}

#[unsafe(no_mangle)]
fn __popcorn_upt_translate_page(this: &AddressSpace, page: RawPage) -> Option<RawFrame> {
	let _guard = this.__assert_in_use();
	let inner = to_inner(this);
	inner.ttable.translate_page(page, false)
}

#[unsafe(no_mangle)]
fn __popcorn_upt_translate_addr(this: &AddressSpace, addr: VirtualAddress) -> Option<PhysicalAddress> {
	let _guard = this.__assert_in_use();
	let inner = to_inner(this);
	inner.ttable.translate_address(addr, false)
}

pub struct MappingEntry<'a> {
	key: MappingKey,
	spinlock: SpinlockGuard<'a, Slab<(Cow<'static, str>, Mapping<Box<dyn Mappable + Send>, Userspace>)>>,
	data: *mut Mapping<Box<dyn Mappable + Send>, Userspace>,
}

impl MappingEntry<'_> {
	pub fn remove(mut self) {
		let res = self.spinlock.try_remove(self.key.0);
		unsafe { res.unwrap_unchecked() };
	}
}

impl Deref for MappingEntry<'_> {
	type Target = Mapping<Box<dyn Mappable + Send>, Userspace>;

	fn deref(&self) -> &Self::Target {
		unsafe { &*self.data }
	}
}

impl DerefMut for MappingEntry<'_> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { &mut *self.data }
	}
}

#[unsafe(no_mangle)]
fn __popcorn_address_space_is_current(_to_check: *const ()) -> bool {
	todo!()
}
