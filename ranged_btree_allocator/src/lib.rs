#![cfg_attr(not(test), no_std)]

#![feature(kernel_virtual_memory)]

use core::ops::Range;
use kernel_api::memory::{AllocError, Page};
use ranged_btree::RangedBTreeMap;
use kernel_api::memory::r#virtual::VirtualAllocator;
use kernel_api::sync::Mutex;

#[derive(Debug)]
struct Meta {
    len: usize
}

pub struct RangedBtreeAllocator {
    range: Range<Page>,
    map: Mutex<RangedBTreeMap<Page, Meta>>
}

impl RangedBtreeAllocator {
    pub fn new(range: Range<Page>) -> Self {
        Self {
            range,
            map: Mutex::new(RangedBTreeMap::new())
        }
    }
}

impl VirtualAllocator for RangedBtreeAllocator {
    fn allocate_contiguous(&self, len: usize) -> Result<Page, AllocError> {
        let mut guard = self.map.lock();

        let Some(first) = guard.first_key() else {
            guard.insert(self.range.start..(self.range.start + len), Meta { len })
                    .expect("BTree should be empty");
            return Ok(self.range.start);
        };

        if (*first - self.range.start) >= len {
            guard.insert(self.range.start..(self.range.start + len), Meta { len })
                 .expect("Just checked this region is free");
            return Ok(self.range.start);
        }

        let last_used = *guard.last_key().expect("Already checked for at least one entry");
        if (self.range.end - last_used) >= len {
            guard.insert(last_used..(last_used + len), Meta { len })
                 .expect("Just checked this region is free");
            return Ok(last_used);
        }

        todo!()
    }

    fn allocate_contiguous_at(&self, at: Page, len: usize) -> Result<Page, AllocError> {
        let mut guard = self.map.lock();
        match guard.insert(at..(at + len), Meta { len }) {
            Ok(_) => Ok(at),
            Err(_) => Err(AllocError)
        }
    }

    fn deallocate_contiguous(&self, base: Page, len: usize) {
        // assumes that deallocations cover an entire allocation

        let mut guard = self.map.lock();
        if let Some(meta) = guard.remove(base) {
            debug_assert_eq!(meta.len, len);
        } else {
            unreachable!("Attempted to deallocate memory that wasn't allocated by this allocator")
        }
    }
}

#[cfg(test)]
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
