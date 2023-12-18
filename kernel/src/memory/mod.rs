pub mod heap;
pub mod r#virtual;
pub mod physical;

use alloc::sync::Arc;
use core::any::Any;
use core::borrow::Borrow;
use core::mem;
use core::mem::MaybeUninit;
use core::num::NonZeroUsize;
use core::ops::{Deref, DerefMut};
use kernel_api::sync::{RwLock, RwReadGuard, RwUpgradableReadGuard, RwWriteGuard};
//pub use kernel_exports::memory::{Frame, Page, PhysicalAddress, VirtualAddress};

use kernel_api::memory::{Frame, PhysicalAddress, allocator::{BackingAllocator, AllocError, GlobalAllocator}};

/*
pub mod alloc {
	pub mod phys {
		use core::alloc::AllocError;
		use core::num::NonZeroUsize;
		use lazy_static::lazy_static;
		use kernel_exports::sync::{PoisonError, RwLock};
		use crate::memory::{AlignedAllocError, Allocator};
		use super::super::Frame;

		mod zero_allocator {
			use core::alloc::AllocError;
			use core::num::NonZeroUsize;
			use core::ptr;
			use kernel_exports::memory::{Frame, Page};

			pub struct GlobalZeroAllocator;

			impl super::Allocator for GlobalZeroAllocator {
				fn allocate_contiguous_aligned(&self, count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AllocError> {
					if false {
						todo!("Have list of pre-zeroed frames")
					} else {
						const PAGE: Page = Page { number: 0xdeadbeef };

						todo!();

						let frame = super::Global.allocate_contiguous_aligned_zeroed(count, alignment_log2)?;
						let mut page_table_guard = crate::memory::paging3::CURRENT_PAGE_TABLE.lock().unwrap();
						//page_table_guard.map_page_to(PAGE, frame, &super::SMOL_ALLOC).expect("help?");
						unsafe { ptr::write_bytes(0xdeadbeef000 as *mut u8, 0, 4096); }
						page_table_guard.unmap_page(PAGE);
						Ok(frame)
					}
				}

				fn allocate_one_aligned_zeroed(&self, alignment_log2: u32) -> Result<Frame, AllocError> { self.allocate_one_aligned(alignment_log2) }
				fn allocate_contiguous_aligned_zeroed(&self, count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AllocError> { self.allocate_contiguous_aligned(count, alignment_log2) }
			}
		}

		pub static GLOBAL_HIGH_MEM_ALLOCATOR: GlobalAllocator = GlobalAllocator(RwLock::new(None));
		pub static GLOBAL_LOW_MEM_ALLOCATOR: GlobalAllocator = GlobalAllocator(RwLock::new(None));
		pub static GLOBAL_ZERO_FRAME_ALLOCATOR: GlobalAllocator = GlobalAllocator(RwLock::new(/*zero_allocator::GlobalZeroAllocator*/ None));

		pub struct GlobalAllocator(RwLock<Option<&'static dyn Allocator>>);

		impl GlobalAllocator {
			pub fn set(&self, allocator: &'static dyn Allocator) {
				let mut guard = self.0.write().unwrap_or_else(PoisonError::into_inner);
				*guard = Some(allocator);
				self.0.unpoison();
			}

			pub fn take(&self) {
				let mut guard = self.0.write().unwrap_or_else(PoisonError::into_inner);
				*guard = None;
				self.0.unpoison();
			}
		}

		pub struct Global;
		pub struct GlobalLowMem;

		impl Global {
			fn lock_high_mem(&self) -> &'static dyn Allocator {
				GLOBAL_HIGH_MEM_ALLOCATOR.0.read().unwrap()
				                         .expect("No global allocator")
			}

			fn lock_zero_frame(&self) -> &'static dyn Allocator {
				/* GLOBAL_ZERO_FRAME_ALLOCATOR */ GLOBAL_HIGH_MEM_ALLOCATOR.0.read().unwrap()
				                         .expect("No global allocator")
			}
		}

		impl Allocator for Global {
			fn allocate_one_aligned_zeroed(&self, alignment_log2: u32) -> Result<Frame, AllocError> { self.lock_zero_frame().allocate_one_aligned_zeroed(alignment_log2) }
			fn allocate_contiguous_aligned(&self, count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AllocError> { self.lock_high_mem().allocate_contiguous_aligned(count, alignment_log2) }
			fn allocate_contiguous(&self, count: NonZeroUsize) -> Result<Frame, AllocError> { self.lock_high_mem().allocate_contiguous(count) }
			fn try_allocate_contiguous_aligned(&self, count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AlignedAllocError> { self.lock_high_mem().try_allocate_contiguous_aligned(count, alignment_log2) }
			fn allocate_contiguous_aligned_zeroed(&self, count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AllocError> { self.lock_zero_frame().allocate_contiguous_aligned_zeroed(count, alignment_log2) }
		}
	}

	pub mod virt {
		use core::alloc::AllocError;
		use core::num::NonZeroUsize;
		use kernel_exports::memory::Frame;
		use crate::memory::{Allocator, Page};
		use crate::memory::alloc::phys::Global;

		#[inline]
		pub fn kpmalloc(page_count: NonZeroUsize) -> Result<Page, AllocError> {
			kpmalloc_with(page_count, Global)
		}

		#[inline]
		pub fn kpmalloc_with(page_count: NonZeroUsize, allocator: impl Allocator) -> Result<Page, AllocError> {
			let backing = allocator.allocate_contiguous(page_count)?;
			kpmalloc_map(backing)
		}

		#[inline]
		pub fn kpmalloc_aligned(page_count: NonZeroUsize, alignment_log2: u32) -> Result<Page, AllocError> {
			kpmalloc_aligned_with(page_count, alignment_log2, Global)
		}

		#[inline]
		pub fn kpmalloc_aligned_with(page_count: NonZeroUsize, alignment_log2: u32, allocator: impl Allocator) -> Result<Page, AllocError> {
			let backing = allocator.allocate_contiguous_aligned(page_count, alignment_log2)?;
			kpmalloc_map(backing)
		}

		fn kpmalloc_map(_backing: Frame) -> Result<Page, AllocError> {
			todo!("map to virtual address space")
		}
	}
}

#[cfg(any())]
pub mod paging2;
pub mod paging3;
pub mod paging {
	use core::marker::PhantomData;
	use core::ptr::NonNull;
	use alloc::rc::Rc;
	use bitflags::bitflags;
	//use super::paging2::messing_about::PageTable;

	//const ACTIVE_PAGE_TABLE: NonNull<TopLevelTable> = unsafe { NonNull::new_unchecked(0o177777_400_400_400_400_0000 as *mut u8).cast() };

	//pub const CURRENT_PAGE_TABLE: ActiveTable = todo!();

	enum RootTable {
		/// The table being modified is the currently active table and is self-mapped at index 256
		SelfMap,
		/// The table being modified is an inactive page table mapped at index 257
		OtherMap
	}

	bitflags! {
		#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
		pub struct EntryFlags: u32 {
			/// Page is accessible
			const PRESENT       = 1<<0;
			/// Page is writable
			const WRITABLE      = 1<<1;
			/// Page will be copied when writing is attempted
			const COPY_ON_WRITE = 1<<2;
			/// Page is swapped out
			const PAGED_OUT     = 1<<9;
			/// Caching is disabled
			const MMIO          = 1<<3;
			/// Cannot execute code from this page
			const NO_EXECUTE    = 1<<4;
			/// Page is accessible from outside the kernel
			const USERSPACE     = 1<<5;
			/// Page has been read since this flag was last cleared
			const ACCESSED      = 1<<6;
			/// Page has been written since this flag was last cleared
			const DIRTY         = 1<<7;
			/// Page does not need to be evicted from TLB on context switch
			const GLOBAL        = 1<<8;
		}
	}

	/*pub struct Mapper<'table> {
		ty: RootTable,
		_phantom: PhantomData<&'table mut TopLevelTable>
	}

	impl<'table> Mapper<'table> {
	}

	pub trait PageTable {
		fn modify_with(&mut self, f: impl FnOnce(Mapper<'_>));
	}

	pub struct ActiveTable(());

	impl PageTable for ActiveTable {
		fn modify_with(&mut self, f: impl FnOnce(Mapper<'_>)) {
			let mapper = Mapper {
				ty: RootTable::SelfMap,
				_phantom: PhantomData
			};
			f(mapper);
		}
	}

	pub struct InactiveTable {
		table: Rc<TopLevelTable> // todo: should this be Rc or something else?
	}

	impl PageTable for InactiveTable {
		fn modify_with(&mut self, f: impl FnOnce(Mapper<'_>)) {

			todo!("map the inactive table to 0o177777_400_400_400_401_0000");
			let mapper = Mapper {
				ty: RootTable::OtherMap,
				_phantom: PhantomData
			};
			f(mapper);
			todo!("unmap the inactive table from 0o177777_400_400_400_401_0000");
		}
	}*/
}*/

#[cfg(test)]
mod tests2 {
	use core::mem::MaybeUninit;
	use core::sync::atomic::{AtomicUsize, Ordering};
	use super::*;

	struct MockAllocator {
		expected_frames: usize,
		actual_frames: AtomicUsize,
		base: Frame,
		always_fail: bool
	}

	impl MockAllocator {
		fn new_fail() -> Self {
			Self {
				always_fail: true,
				.. Default::default()
			}
		}

		fn new_with_frames(expected: usize) -> Self {
			Self {
				expected_frames: expected,
				.. Default::default()
			}
		}

		fn verify(&self) {
			let actual = self.actual_frames.load(Ordering::Acquire);
			if actual != self.expected_frames {
				panic!("{actual} frames allocated when {} were expected", self.expected_frames);
			}
		}
	}

	impl Default for MockAllocator {
		fn default() -> Self {
			Self {
				expected_frames: Default::default(),
				actual_frames: Default::default(),
				base: Frame::new(PhysicalAddress::new(0)),
				always_fail: false
			}
		}
	}

	unsafe impl BackingAllocator for MockAllocator {
		fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, AllocError> {
			if self.always_fail { return Err(AllocError); }

			let old_frame_count = self.actual_frames.fetch_add(frame_count, Ordering::Release);
			if (old_frame_count + frame_count) > self.expected_frames {
				panic!("Attempted to allocate {} frames when only {} were expected", old_frame_count + frame_count, self.expected_frames);
			}

			Ok(self.base + old_frame_count)
		}

		unsafe fn deallocate_contiguous(&self, _: Frame, _: NonZeroUsize) {}
	}

	#[test]
	fn mock_sanity() {
		let mock = MockAllocator::new_fail();
		assert!(mock.allocate_contiguous(4).is_err());

		let mock = MockAllocator::new_with_frames(5);
		assert!(mock.allocate_contiguous(3).is_ok());
		assert!(mock.allocate_contiguous(2).is_ok());
		mock.verify();
	}
/*
	#[test_case]
	fn allocate_returns_correct_number() {
		let mock = MockAllocator::new_with_frames(5);
		let allocation = mock.allocate_contiguous(5);
		let len = allocation
			.inspect(|a| assert!(a.is_ok()))
			.count();
		assert_eq!(len, 5);
		mock.verify();
	}*/

	/*#[test_case]
	fn allocations_do_not_overlap() {
		let mut allocation_store = MaybeUninit::uninit_array::<5>();
		let mock = MockAllocator::new_with_frames(5);
		let allocations = mock.allocate_contiguous(5);
		for (i, allocation) in allocations.enumerate() {
			assert!(allocation.is_ok());
			allocation_store[i].write(allocation.unwrap());
		}
		mock.verify();

		let allocation_store = unsafe { MaybeUninit::array_assume_init(allocation_store) };
		for (i, frame) in allocation_store.iter().enumerate() {
			assert!(!allocation_store[..i].contains(frame));
		}
	}*/

	#[test]
	fn allocate_one_allocates_one() {
		let mock = MockAllocator::new_with_frames(1);
		assert!(mock.allocate_one().is_ok());
		mock.verify();
	}
}
