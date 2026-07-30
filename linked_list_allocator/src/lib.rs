//! A virtual memory allocator which stores allocations in an ordered linked list.
//!
//! # Examples
//!
//! ```
//! use kernel_api::memory::{RawPage, PAGE_SIZE};
//! use linked_list_allocator::LinkedListAllocator;
//!
//! // create an allocator covering the first 8 pages of memory
//! let mut allocator = LinkedListAllocator::new(RawPage::new(0)..RawPage::new(8 * PAGE_SIZE)).unwrap();
//!
//! // mark holes in the address space as unusable
//! allocator.add_allocations([
//!     RawPage::new(2 * PAGE_SIZE)..RawPage::new(3 * PAGE_SIZE),
//!     RawPage::new(5 * PAGE_SIZE)..RawPage::new(7 * PAGE_SIZE),
//! ]);
//!
//! // allocate some pages
//! let alloc_page = allocator.allocate_contiguous(1).unwrap();
//! let alloc_in_position = allocator.allocate_contiguous_at(RawPage::new(3 * PAGE_SIZE), 2).unwrap();
//!
//! // deallocate the pages
//! allocator.deallocate_contiguous(alloc_page, 1);
//! allocator.deallocate_contiguous(alloc_in_position, 2);
//! ```

#![feature(gen_blocks)]
#![feature(strict_provenance_lints)]
#![cfg_attr(not(test), no_std)]
#![feature(int_roundings)]

#![cfg_attr(test, allow(unused_imports))]
#![cfg_attr(doc, feature(rustdoc_missing_doc_code_examples))]

#[cfg(test)] extern crate alloc;

mod linked_list;

use linked_list::{LinkedList, Meta};
use core::cmp::{max, min};
use core::num::NonZero;
use core::ops::Range;
use log::{debug, trace};
use kernel_api::memory::{RawPage, PAGE_SIZE};
use kernel_api::sync::Spinlock;
use kernel_api::allocator::{Vmm, AllocError};
use kernel_api::memory::asan::{asan_free_range, set_shadow_free_vmem, mem_to_shadow, count_to_shadow};

/// A virtual memory allocator using a singly linked list to store allocation metadata.
///
/// `LinkedListAllocator` will allocate from the [`highmem`](kernel_api::memory#highmem) allocator
/// once on creation. After initialisation, the maximum number of allocations that can be active
/// simulatenously is fixed.
///
/// See the [crate-level documentation](crate) for more information.
#[expect(rustdoc::missing_doc_code_examples, reason = "example in crate level docs")]
#[derive(Debug)]
pub struct LinkedListAllocator {
    range: Range<RawPage>,
    list: Spinlock<LinkedList>,
}

impl LinkedListAllocator {
    /// Creates a new `LinkedListAllocator` which will allocate pages in the specified `range`.
    ///
    /// An allocation will be made from the [`highmem`](kernel_api::memory#highmem) allocator to
    /// store metadata for the allocator in.
    /// The number of frames allocated from `highmem` is an unspecified implementation detail.
    ///
    /// # Errors
    ///
    /// Returns [`AllocError`] if memory to hold the metadata could not be allocated.
    ///
    /// # Examples
    ///
    /// ```
    /// use kernel_api::memory::{RawPage, PAGE_SIZE};
    /// use linked_list_allocator::LinkedListAllocator;
    ///
    /// let allocator = LinkedListAllocator::new(RawPage::new(0)..RawPage::new(PAGE_SIZE)).unwrap();
    /// ```
    pub fn new(range: Range<RawPage>) -> Result<Self, AllocError> {
	    let allocation_count = if range.is_empty() {
		    const { NonZero::new(1).unwrap() }
	    } else {
		    const { NonZero::new(PAGE_SIZE).unwrap() } // mostly going to be kernel stacks so lets say ~4k threads?
	    };

        Ok(Self {
            range,
            list: Spinlock::new(LinkedList::new(allocation_count)?),
        })
    }

    /// Marks regions of the `LinkedListAllocator` as already being allocated.
    ///
    /// The passed set of `allocations` is iterated over, and each one is inserted into the list of
    /// allocations made. If iterating causes a panic, any ranges yielded from the iterator before
    /// the panic will remain allocated.
    ///
    /// <div class="warning">
    ///
    /// If two ranges in `allocations` overlap, only the first range will be marked as allocated
    /// and the second will be silently ignored.
    ///
    /// </div>
    ///
    /// # Examples
    ///
    /// ```
    /// # use kernel_api::allocator::AllocError;
    /// use kernel_api::memory::{RawPage, PAGE_SIZE};
    /// use linked_list_allocator::LinkedListAllocator;
    ///
    /// let mut allocator = LinkedListAllocator::new(RawPage::new(0)..RawPage::new(PAGE_SIZE))?;
    /// allocator.add_allocations([RawPage::new(0)..RawPage::new(PAGE_SIZE)]);
    /// assert!(allocator.allocate_contiguous(1).is_err());
    /// # Ok::<(), AllocError>::(())
    /// ```
    pub fn add_allocations(&mut self, allocations: impl IntoIterator<Item = Range<RawPage>>) {
        let guard = self.list.get_mut();

        for allocation in allocations {
            let isect = max(allocation.start, self.range.start)..min(allocation.end, self.range.end);
            if !isect.is_empty() {
                debug!("insert allocation at {isect:#x?}");
                let _ = guard.insert(isect.clone(), Meta { len: isect.end - isect.start });
            }
        }
    }
}

impl Vmm for LinkedListAllocator {
    fn allocate_contiguous(&self, len: usize) -> Result<RawPage, AllocError> {
        let mut guard = self.list.lock();
        
        let gap = 'iter: {
            for gap in guard.iter_gaps(self.range.clone()) {
                let gap_len = gap.end - gap.start;
                if gap_len >= len {
                    break 'iter gap;
                }
            }

            return Err(AllocError::vmm());
        };

        guard.insert(
            gap.start..gap.start + len,
            Meta { len },
        ).map_err(|_| AllocError::vmm())?;

	    trace!("{:#x?} {:?}", gap.start..gap.start + len, Meta { len });

        if self.range.start.is_higher_half() {
            asan_free_range(
                gap.start.into(),
                len * PAGE_SIZE,
            );
        }

        Ok(gap.start)
    }

    fn allocate_contiguous_at(&self, at: RawPage, len: usize) -> Result<RawPage, AllocError> {
        if at < self.range.start { return Err(AllocError::vmm()); }
        if (at + len) > self.range.end { return Err(AllocError::vmm()); }
        
        let mut guard = self.list.lock();
        match guard.insert(
            at..at + len,
            Meta { len },
        ) {
            Ok(()) => {
                drop(guard);
                if at.is_higher_half() {
                    asan_free_range(
                        at.into(),
                        len * PAGE_SIZE,
                    );
                }
                Ok(at)
            },
            Err(_) => Err(AllocError::vmm())
        }
    }

    fn deallocate_contiguous(&self, base: RawPage, len: usize) {
        // assumes that deallocations cover an entire allocation

        // SAFETY: `mem_to_shadow` returns valid addresses in the shadow region
        unsafe {
            if base.is_higher_half() {
                set_shadow_free_vmem(
                    mem_to_shadow(base.into()),
                    count_to_shadow(len * PAGE_SIZE),
                );
            }
        }

        let mut guard = self.list.lock();
        if let Some(meta) = guard.remove(base.into()) {
            debug_assert_eq!(meta.len, len, "`len` passed to deallocate_contiguous should match original allocation length");
        } else {
            unreachable!("Attempted to deallocate memory that wasn't allocated by this allocator")
        }
    }
}

#[cfg(any())]
mod tests {
    use kernel_api::memory::VirtualAddress;
    use super::*;

    const START: Page = Page::new(VirtualAddress::new(0x1_0000));
    const END: Page = Page::new(VirtualAddress::new(0x10_0000));

    #[test]
    fn allocate_in_empty() {
        let allocator = LinkedListAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);
    }

    #[test]
    fn cannot_overallocate() {
        let allocator = LinkedListAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);

        allocator.allocate_contiguous_at(START, 3).expect_err("Allocation is already allocated");
    }

    #[test]
    fn allocate_multiple() {
        let allocator = LinkedListAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START + 5);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START + 10);
    }

    #[test]
    fn allocate_and_deallocate() {
        let allocator = LinkedListAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START + 5);

        allocator.deallocate_contiguous(START, 5);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);
    }

    #[test]
    #[should_panic]
    fn allocator_length_sanity() {
        let allocator = LinkedListAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);
        allocator.deallocate_contiguous(START, 3);
    }

    #[test]
    #[should_panic]
    fn allocator_allocation_sanity() {
        let allocator = LinkedListAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);
        allocator.deallocate_contiguous(START + 5, 3);
    }
}
