#![feature(allocator_api)]
#![feature(gen_blocks)]
#![cfg_attr(not(test), no_std)]
#![feature(step_trait)]
#![feature(int_roundings)]
#![feature(unsigned_nonzero_div_ceil)]
#![deny(warnings)]

#![cfg_attr(test, allow(unused_imports))]

#[cfg(test)] extern crate alloc;

mod linked_list;

use linked_list::{LinkedList, Meta};
use core::cmp::{max, min};
use core::num::NonZero;
use core::ops::Range;
use log::{debug, trace};
use kernel_api::memory::RawPage;
use kernel_api::sync::Spinlock;
use kernel_api::allocator::{Vmm, AllocError};
#[cfg(feature = "kasan")] use kernel_api::memory::asan::{asan_free_range, set_shadow_free_vmem, mem_to_shadow, count_to_shadow};

#[derive(Debug)]
pub struct RangedBtreeAllocator {
    range: Range<RawPage>,
    list: Spinlock<LinkedList>,
}

impl RangedBtreeAllocator {
    pub fn new(range: Range<RawPage>) -> Result<Self, AllocError> {
	    let allocation_count = if range.is_empty() {
		    const { NonZero::new(1).unwrap() }
	    } else {
		    const { NonZero::new(4096).unwrap() } // mostly going to be kernel stacks so lets say ~4k threads?
	    };

        Ok(Self {
            range,
            list: Spinlock::new(LinkedList::new(allocation_count)?),
        })
    }

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

impl Vmm for RangedBtreeAllocator {
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

        #[cfg(feature = "kasan")] if self.range.start.is_higher_half() {
            asan_free_range(
                gap.start.into(),
                len * 4096,
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
            Ok(_) => {
                drop(guard);
                #[cfg(feature = "kasan")] if at.is_higher_half() {
                    asan_free_range(
                        at.into(),
                        len * 4096,
                    );
                }
                Ok(at)
            },
            Err(_) => Err(AllocError::vmm())
        }
    }

    fn deallocate_contiguous(&self, base: RawPage, len: usize) {
        // assumes that deallocations cover an entire allocation

        #[cfg(feature = "kasan")] unsafe {
            if base.is_higher_half() {
                set_shadow_free_vmem(
                    mem_to_shadow(base.into()),
                    count_to_shadow(len * 4096),
                );
            }
        }

        let mut guard = self.list.lock();
        if let Some(meta) = guard.remove(base.into()) {
            debug_assert_eq!(meta.len, len);
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
        let allocator = RangedBtreeAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);
    }

    #[test]
    fn cannot_overallocate() {
        let allocator = RangedBtreeAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);

        allocator.allocate_contiguous_at(START, 3).expect_err("Allocation is already allocated");
    }

    #[test]
    fn allocate_multiple() {
        let allocator = RangedBtreeAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START + 5);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START + 10);
    }

    #[test]
    fn allocate_and_deallocate() {
        let allocator = RangedBtreeAllocator::new(START..END);

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
        let allocator = RangedBtreeAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);
        allocator.deallocate_contiguous(START, 3);
    }

    #[test]
    #[should_panic]
    fn allocator_allocation_sanity() {
        let allocator = RangedBtreeAllocator::new(START..END);

        let allocation = allocator.allocate_contiguous(5).expect("Allocation should not fail");
        assert_eq!(allocation, START);
        allocator.deallocate_contiguous(START + 5, 3);
    }
}
