//! Provides physical memory allocation APIs

#![stable(feature = "kernel_core_api", since = "0.1.0")]

use core::num::{NonZeroU32, NonZeroUsize};
use core::ops::Range;
use auto_impl::auto_impl;

use super::{Frame, AllocError, PAGE_SIZE};

/// The error returned when an allocation requesting zeroed out memory was unsuccessful
#[unstable(feature = "kernel_allocation_zeroing", issue = "2")]
#[derive(Debug, Eq, PartialEq)]
pub enum ZeroAllocError {
    /// No allocation could be made
    AllocError,
    /// An allocation could be made, but the result is not zeroed
    Uninit(Frame),
}

#[unstable(feature = "kernel_allocation_zeroing", issue = "2")]
#[doc(hidden)]
impl From<AllocError> for ZeroAllocError {
    fn from(_: AllocError) -> Self { Self::AllocError }
}

#[unstable(feature = "kernel_allocation_new", issue = "5")]
pub struct Config<'a> {
    pub allocation_range: Range<Frame>,
    pub regions: &'a mut dyn Iterator<Item = Range<Frame>>
}

#[unstable(feature = "kernel_allocation_new", issue = "5")]
#[non_exhaustive]
pub struct AllocationMeta {
    pub region: Range<Frame>
}

impl AllocationMeta {
    #[unstable(feature = "kernel_allocation_new", issue = "5")]
    pub fn new(region: Range<Frame>) -> Self {
        Self { region }
    }
}

#[unstable(feature = "kernel_physical_allocator_non_contiguous", issue = "none")]
pub type AllocateNonContiguousRet = impl IntoIterator<Item = Frame>;

const _: () = {
    fn dummy() -> AllocateNonContiguousRet {
        fn f() -> Range<Frame> { unimplemented!() }
        f()
    }
};

/// An allocator that managed physical memory
///
/// In future, this may be replaced by a more general resource allocator. In that case, this trait will be deprecated
/// and implementors will be expected to move to the more general trait.
#[auto_impl(&, Box, Arc)]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub unsafe trait BackingAllocator: Send + Sync {
    // (Bitmap, Buddy, Watermark, ...)

    // UNRESOLVED: how should errors work into this - does it error early and somehow figure out ahead of time if there's enough free space for the entire allocate, or does the allocation itself happen lazily and so an alloc error can happen on each iteration. If the latter, what happens if you call next() after getting an alloc error?
    // Allocates `frame_count` frames of physical memory, not necessarily contiguously.
    // Returns an iterator over the allocated frames.
    // It is undecided for now whether allocation should be done upfront or lazily as the iterator is polled.
    //
    // Implementations should only implement this when there is a fast path compared to [`allocate_contiguous`](Self::allocate_contiguous).
    // Otherwise the default implementation will use [`allocate_contiguous`](Self::allocate_contiguous).
    /*fn allocate(&self, frame_count: usize) -> impl Iterator<Item = Result<Frame, AllocError>> where Self: Sized {
        gen move {
            let base = self.allocate_contiguous(frame_count);
            if let Ok(base) = base {
                for i in 0..frame_count { yield Ok(base + i); }
            } else { yield Err(AllocError); }
        }
    }*/

    #[unstable(feature = "kernel_physical_allocator_non_contiguous", issue = "none")]
    fn allocate(&self, frame_count: usize) -> Result<AllocateNonContiguousRet, AllocError> {
        let base = self.allocate_contiguous(frame_count)?;
        Ok(base..(base + frame_count))
    }

    /// Allocates a contiguous range of physical memory
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, AllocError>;

    /// Allocates a single [`Frame`]
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    fn allocate_one(&self) -> Result<Frame, AllocError> {
        let frame = self.allocate(1)?;
        Ok(frame.into_iter().next().expect("`allocate(1)` must return one frame"))
    }

    /// Tries to allocate a contiguous region of `frame_count` frames from a prezeroed buffer
    #[unstable(feature = "kernel_allocation_zeroing", issue = "2")]
    fn try_allocate_zeroed(&self, frame_count: usize) -> Result<Frame, ZeroAllocError> { Err(ZeroAllocError::Uninit(self.allocate_contiguous(frame_count)?)) }

    /// Allocates a contiguous region of `frame_count` frames, manually zeroing them if there were no prezeroed frames
    #[unstable(feature = "kernel_allocation_zeroing", issue = "2")]
    fn allocate_zeroed(&self, frame_count: usize) -> Result<Frame, AllocError> {
        #[cold]
        fn do_zero(frame: Frame, frame_count: usize) {
            let page = frame.to_page();

            unsafe {
                core::ptr::write_bytes(
                    page.as_ptr(),
                    0,
                    frame_count * PAGE_SIZE
                );
            }
        }

        let frames = self.try_allocate_zeroed(frame_count);
        match frames {
            Ok(frame) => Ok(frame),
            Err(ZeroAllocError::AllocError) => Err(AllocError),
            Err(ZeroAllocError::Uninit(frame)) => {
                do_zero(frame, frame_count);
                Ok(frame)
            }
        }
    }

    /// # Safety
    /// Must be deallocated with the same allocator that made the allocation
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    unsafe fn deallocate_contiguous(&self, base: Frame, frame_count: NonZeroUsize);

    #[unstable(feature = "kernel_allocation_new", issue = "5")]
    fn push(&mut self, allocation: AllocationMeta) { unimplemented!("experimental") }

    #[unstable(feature = "kernel_physical_allocator_location", issue = "none")]
    fn allocate_at(&self, frame_count: usize, location: SpecificLocation) -> Result<Frame, AllocError>;
}

#[unstable(feature = "kernel_allocation_new", issue = "5")]
pub unsafe trait SizedBackingAllocator: BackingAllocator + Sized {
    #[unstable(feature = "kernel_allocation_new", issue = "5")]
    fn new(config: Config) -> &'static mut dyn BackingAllocator;
}

#[unstable(feature = "kernel_physical_allocator_location", issue = "none")]
pub enum SpecificLocation {
    /// The mapping must be aligned to a specific number of [`Frame`]s
    Aligned(NonZeroU32),
    /// The mapping will fail if it cannot be allocated at this exact location
    At(Frame),
    /// The mapping must be below this location, aligned to `with_alignment` number of [`Frame`]s
    Below { location: Frame, with_alignment: NonZeroU32 }
}

#[unstable(feature = "kernel_physical_allocator_location", issue = "none")]
pub enum Location {
    Any,
    Specific(SpecificLocation)
}

#[unstable(feature = "kernel_physical_allocator_location", issue = "none")]
pub enum AlignError {
    OomError,
    Unaligned(AllocateNonContiguousRet)
}





