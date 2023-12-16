use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::num::NonZeroUsize;
use
use kernel_api::memory::allocator::{BackingAllocator, AllocError, ZeroAllocError, AllocatorConfig};

pub struct BackgroundZeroer<A: BackingAllocator> {
    zero_frames: Vec<Frame>,
    backing: A
}

impl<A: BackingAllocator> BackgroundZeroer<A> {
    pub fn new(backing: A) -> Self {
        Self {
            zero_frames: vec![],
            backing
        }
    }
}

impl<A: BackingAllocator> BackingAllocator for BackgroundZeroer<A> {
    fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, AllocError> {
        self.backing.allocate_contiguous(frame_count)
    }

    fn allocate_one(&self) -> Result<Frame, AllocError> {
        self.backing.allocate_one()
    }

    fn try_allocate_zero(&self, frame_count: usize) -> Result<Frame, ZeroAllocError> {
        // TODO: pull from zero list somehow lock free
        self.backing.try_allocate_zero(frame_count)
    }

    unsafe fn deallocate_contiguous(&self, base: Frame, frame_count: NonZeroUsize) {
        unsafe { self.backing.deallocate_contiguous(base, frame_count) }
    }

    fn new(_: AllocatorConfig, _: Option<crate::memory::BackingAllocatorNewFn>) -> Arc<dyn BackingAllocator> {
        todo!()
    }
}
