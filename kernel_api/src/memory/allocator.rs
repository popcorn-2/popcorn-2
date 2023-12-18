//! Provides physical memory allocation APIs

#![stable(feature = "kernel_core_api", since = "0.1.0")]

use alloc::sync::Arc;
use core::num::NonZeroUsize;
use core::ops::Range;
use auto_impl::auto_impl;

use super::Frame;
use super::PAGE_SIZE;

/// The error returned when an allocation was unsuccessful
#[stable(feature = "kernel_core_api", since = "0.1.0")]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AllocError;

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
pub struct AllocationMeta {
    pub region: Range<Frame>
}

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

    /// Allocates a contiguous range of physical memory
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, AllocError>;

    /// Allocates a single [`Frame`]
    #[stable(feature = "kernel_core_api", since = "0.1.0")]
    fn allocate_one(&self) -> Result<Frame, AllocError> {
        // FIXME: Use non-contig version
        self.allocate_contiguous(1)
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
    fn new(config: Config) -> Arc<dyn BackingAllocator> where Self: Sized { unimplemented!("experimental") }

    #[unstable(feature = "kernel_allocation_new", issue = "5")]
    fn push(&mut self, allocation: AllocationMeta) { unimplemented!("experimental") }

    #[unstable(feature = "kernel_allocation_new", issue = "5")]
    fn drain_into(&mut self, into: &mut dyn BackingAllocator) { unimplemented!("experimental") }

    /// Allocate a continuous range of `count` frames, aligned to 2^`alignment_log2` frames
    ///
    /// # Errors
    /// Returns an [`AllocError`](core::alloc::AllocError) if the allocation could not be made
    #[unstable(feature = "kernel_allocation_aligned", issue = "3")]
    fn allocate_contiguous_aligned(&self, count: NonZeroUsize, alignment_log2: u32) -> Result<Frame, AllocError> { unimplemented!("unstable feature") }

    /// Allocate a continuous range of `count` frames.
    /// If the alignment of 2^`alignment_log2` frames cannot be satisfied, make an allocation of the same size with a lower alignment.
    /// The number of allocated frames shall be rounded up to the returned alignment
    ///
    /// # Errors
    /// Returns an [`AlignedAllocError`] if the originally requested alignment could not be satisfied.
    /// The new alignment shall be returned in [`AlignError`](AlignedAllocError::AlignError).
    #[unstable(feature = "kernel_allocation_aligned", issue = "3")]
    fn try_allocate_contiguous_aligned(&self, count: NonZeroUsize, mut alignment_log2: u32) -> Result<Frame, AlignedAllocError> {
        let alignment_log2_orig = alignment_log2;
        loop {
            let alignment = 1 << alignment_log2;
            let aligned_count = (count.get() + alignment - 1) / alignment * alignment;

            // SAFETY: `aligned_count` cannot be zero unless `count` or `alignment` are equal to zero, neither of which is possible
            match self.allocate_contiguous_aligned(unsafe { NonZeroUsize::new_unchecked(aligned_count) }, alignment_log2) {
                Ok(frame) if alignment_log2 == alignment_log2_orig => return Ok(frame),
                Ok(frame) => return Err(AlignedAllocError::AlignError(frame, alignment_log2)),
                _ => {}
            }

            if let Some(new_alignment_log2) = alignment_log2.checked_sub(1) {
                alignment_log2 = new_alignment_log2;
            } else { return Err(AlignedAllocError::AllocError) }
        }
    }
}

/// The error returned when an allocation with a requested alignment could not be satisfied
#[unstable(feature = "kernel_allocation_aligned", issue = "3")]
pub enum AlignedAllocError {
    /// No allocation could be satisfied
    AllocError,
    /// An allocation with different alignment was returned instead
    AlignError(
        /// The start of the allocation
        Frame,
        /// The log_2 of the alignment
        u32
    )
}

#[doc(hidden)]
#[stable(feature = "kernel_core_api", since = "0.1.0")]
pub enum GlobalAllocator {
    Static(&'static dyn BackingAllocator),
    Arc(Arc<dyn BackingAllocator>)
}

#[stable(feature = "kernel_core_api", since = "0.1.0")]
unsafe impl BackingAllocator for GlobalAllocator {
    fn allocate_contiguous(&self, frame_count: usize) -> Result<Frame, AllocError> {
        match self {
            Self::Static(a) => a.allocate_contiguous(frame_count),
            Self::Arc(a) => a.allocate_contiguous(frame_count)
        }
    }

    fn allocate_one(&self) -> Result<Frame, AllocError> {
        match self {
            Self::Static(a) => a.allocate_one(),
            Self::Arc(a) => a.allocate_one()
        }
    }

    fn try_allocate_zeroed(&self, frame_count: usize) -> Result<Frame, ZeroAllocError> {
        match self {
            Self::Static(a) => a.try_allocate_zeroed(frame_count),
            Self::Arc(a) => a.try_allocate_zeroed(frame_count)
        }
    }

    fn allocate_zeroed(&self, frame_count: usize) -> Result<Frame, AllocError> {
        match self {
            Self::Static(a) => a.allocate_zeroed(frame_count),
            Self::Arc(a) => a.allocate_zeroed(frame_count)
        }
    }

    unsafe fn deallocate_contiguous(&self, base: Frame, frame_count: NonZeroUsize) {
        match self {
            Self::Static(a) => a.deallocate_contiguous(base, frame_count),
            Self::Arc(a) => a.deallocate_contiguous(base, frame_count)
        }
    }
}
