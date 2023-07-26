#![feature(allocator_api)]
#![feature(slice_ptr_get)]
#![cfg_attr(not(test), no_std)]

extern crate kernel_exports;
extern crate alloc;

use alloc::alloc::Global;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::alloc::{Allocator, AllocError, Layout};
use core::cell::RefCell;
use core::fmt::Formatter;
use core::mem;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut, Range};
use core::ptr::slice_from_raw_parts_mut;
use kernel_exports::memory::{Frame, PhysicalMemoryAllocator};
use kernel_exports::sync::Lock;
use utils::handoff::MemoryMapEntry;
use crate::memory_map::MemoryMap;

/*module_name!("Bitmap Memory Allocator", "popcorn::memory::bitmap_alloc");
module_author!("Eliyahu Gluschove-Koppel <popcorn@eliyahu.co.uk>");
module_license!("MPL-2.0");

#[module_main(allocator(general))]
fn main(allocator: &dyn PhysicalMemoryAllocator, coverage: Range<Frame>) -> Result<BitmapAllocator, ()> {
    BitmapAllocator::try_new(allocator, coverage).map_err(|_| ())
}*/

mod memory_map {
    use core::ops::{Deref, DerefMut};
    use kernel_exports::memory::PhysicalMemoryAllocator;

    pub struct MemoryMap;

    impl MemoryMap {
        pub fn new(page_count: usize) -> Self {
            todo!()
        }

        pub fn zeroed_with(page_count: usize, allocator: &mut dyn PhysicalMemoryAllocator) -> Self {
            todo!()
        }
    }

    impl Deref for MemoryMap {
        type Target = [u8];
        fn deref(&self) -> &Self::Target {
            todo!()
        }
    }

    impl DerefMut for MemoryMap {
        fn deref_mut(&mut self) -> &mut Self::Target {
            todo!()
        }
    }
}

struct BitmapAllocator {
    range: Range<Frame>,
    bitmap: Lock<MemoryMap>
}

impl core::fmt::Debug for BitmapAllocator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BitmapAllocator")
                .field("range", &self.range)
                .field("bitmap", &"[..]")
                .finish()
    }
}

enum FrameState { Allocated, Free }

impl BitmapAllocator {
    const FRAMES_PER_ENTRY: usize = 8 * mem::size_of::<usize>();

    fn try_set_frame(&self, frame: Frame, state: FrameState) -> Result<(), ()> {
        let mut bmp = self.bitmap.lock();

        let (bitmap_index, bit_index) = self.frame_to_indices(frame);

        if !self.range.contains(&frame) {
            return Err(());
        }

        match state {
            FrameState::Allocated => bmp[bitmap_index] &= !(1usize << bit_index),
            FrameState::Free => bmp[bitmap_index] |= (1usize << bit_index)
        }

        Ok(())
    }

    fn try_get_frame(&self, frame: Frame) -> Result<FrameState, ()> {
        let (bitmap_index, bit_index) = self.frame_to_indices(frame);

        if !self.range.contains(&frame) {
            return Err(());
        }

        return Ok(if (self.bitmap.lock()[bitmap_index] & (1usize << bit_index)) != 0 {
            FrameState::Allocated
        } else { FrameState::Free });
    }

    fn frame_to_indices(&self, frame: Frame) -> (usize, usize) {
        let number_within_bitmap = frame - self.range.start;
        let bitmap_index         = number_within_bitmap / Self::FRAMES_PER_ENTRY;
        let bit_index            = number_within_bitmap % Self::FRAMES_PER_ENTRY;
        (bitmap_index, bit_index)
    }

    pub fn new_with(coverage: Range<Frame>, allocator: &mut dyn PhysicalMemoryAllocator) -> Self {
        let page_count = coverage.end - coverage.start;
        let new_allocator = Self {
            range: coverage,
            bitmap: Lock::new(MemoryMap::zeroed_with(page_count, allocator))
        };

        for frame in allocator.get_free_regions().flatten() {
            // ignore result since just want to ignore any out of bounds free regions
            let _ = new_allocator.try_set_frame(frame, FrameState::Free);
        }

        new_allocator
    }
}

impl PhysicalMemoryAllocator for BitmapAllocator {
    fn try_new(allocator: &dyn PhysicalMemoryAllocator, coverage: Range<Frame>) -> Result<Self, ()> {
        let page_count = coverage.end - coverage.start;


        Ok(Self {
            range: coverage,
            bitmap: Lock::new(usize_slice)
        })
    }

    fn allocate_contiguous(&self, page_count: usize) -> Result<Frame, ()> {
        if page_count != 1 { return Err(()); }

        for (i, entry) in self.bitmap.lock().iter_mut().enumerate() {
            let first_set_bit = usize::try_from(entry.trailing_zeros()).unwrap();
            if first_set_bit != Self::FRAMES_PER_ENTRY {
                *entry &= !(1usize << first_set_bit);
                let bits_to_start = i * Self::FRAMES_PER_ENTRY;
                return Ok(self.range.start + bits_to_start + first_set_bit);
            }
        }

        return Err(());
    }
}

#[cfg(test)]
mod tests {
    use core::mem;
    use kernel_exports::memory::{Frame, PhysicalMemoryAllocator};
    use crate::BitmapAllocator;

    #[test]
    fn allocates_all_memory() {
        allocates_all_memory_param( 1 * BitmapAllocator::FRAMES_PER_ENTRY );
        allocates_all_memory_param(  2 * BitmapAllocator::FRAMES_PER_ENTRY);
        allocates_all_memory_param(  3 * BitmapAllocator::FRAMES_PER_ENTRY );
        allocates_all_memory_param(  4 * BitmapAllocator::FRAMES_PER_ENTRY);
        allocates_all_memory_param( 5 * BitmapAllocator::FRAMES_PER_ENTRY);
    }

    fn make_allocator(size: usize) -> BitmapAllocator {
        let mut allocator = BitmapAllocator::try_new(Frame(0) .. Frame(size), &std::alloc::Global).unwrap();
        allocator.bitmap.lock().fill(usize::MAX);
        allocator
    }

    fn allocates_all_memory_param(size: usize) {
        let allocator = make_allocator(size);

        for i in 0..size {
            assert!(allocator.allocate_contiguous(1).is_ok(), "Failed to allocate on iteration {i} of {N}");
        }
        assert!(allocator.allocate_contiguous(1).is_err());
    }
}
