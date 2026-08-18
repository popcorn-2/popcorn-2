//! A physical memory allocator which tracks allocated pages with an internal bitmap.

#![cfg_attr(not(test), no_std)]
#![feature(strict_provenance_lints)]
#![feature(const_trait_impl)]
#![cfg_attr(doc, feature(rustdoc_missing_doc_code_examples))]

use core::debug_assert_matches;
use core::mem::MaybeUninit;
use core::num::NonZero;
use core::ops::Range;
use kernel_api::memory::{RawFrame, Frames, PAGE_SIZE};
use kernel_api::sync::Spinlock;
use log::debug;
use kernel_api::allocator::{highmem, Pmm, AllocError};

const BITS_PER_BITMAP_UNIT: usize = usize::BITS as usize;

macro_rules! alloc_err {
    ($this:ident, $reason:literal) => {
        debug!(concat!("BitmapAllocator: ", $reason));
        return Err(AllocError::pmm().into());
    };

    ($this:ident, $reason:literal, $($arg:tt)+) => {
        debug!(concat!("BitmapAllocator: ", $reason), $($arg)+);
        return Err(AllocError::pmm().into());
    };
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
enum FrameState {
    Allocated,
    Free
}

#[derive(Debug)]
struct OutOfRangeError;

#[derive(Debug)]
struct BitmapAllocatorInner {
    first_frame: RawFrame,
    bitmap: Frames<true, usize>,
}

impl BitmapAllocatorInner {
    fn last_frame(&self) -> RawFrame {
        self.first_frame + self.bitmap.get().len() * BITS_PER_BITMAP_UNIT
    }

	/// # Errors
	///
	/// Returns [`OutOfRangeError`] if `frame` is out of bounds of this allocator.
	fn set_frame(&mut self, frame: RawFrame, state: FrameState) -> Result<(), OutOfRangeError> {
        if (frame < self.first_frame) || (frame >= self.last_frame()) { return Err(OutOfRangeError); }

        let (bitmap_index, bit_index) = self.frame_to_indices(frame);

        match state {
            FrameState::Allocated => self.bitmap.get_mut()[bitmap_index] &= !(1 << bit_index),
            FrameState::Free => self.bitmap.get_mut()[bitmap_index] |= 1 << bit_index,
        }

        Ok(())
    }

	/// # Errors
	///
	/// Returns [`OutOfRangeError`] if `frame` is out of bounds of this allocator.
    fn get_frame(&self, frame: RawFrame) -> Result<FrameState, OutOfRangeError> {
        if (frame < self.first_frame) || (frame >= self.last_frame()) { return Err(OutOfRangeError); }

        let (bitmap_index, bit_index) = self.frame_to_indices(frame);
        if (self.bitmap.get()[bitmap_index] & (1 << bit_index)) == 0 { Ok(FrameState::Allocated) }
        else { Ok(FrameState::Free) }
    }

	/// # Errors
	///
	/// Returns [`AllocError`] on memory allocation failure for the bitmap.
    fn new(first_frame: RawFrame, frame_count: NonZero<usize>) -> Result<Self, AllocError> {
        let bitmap_length = frame_count.div_ceil(
            const { NonZero::new(8 * PAGE_SIZE).unwrap() }
        );
        let bitmap = highmem().allocate(bitmap_length)?;
        let bitmap = bitmap.cast::<usize>().into_filed(0);

        Ok(Self {
            first_frame,
            bitmap
        })
    }

	/// # Panics
	///
	/// If `frame` is out of bounds of this allocator.
    fn frame_to_indices(&self, frame: RawFrame) -> (usize, usize) {
        assert!(frame >= self.first_frame, "frame is out of bounds of this allocator");

        let number_in_bitmap = frame - self.first_frame;
        let bitmap_index = number_in_bitmap / BITS_PER_BITMAP_UNIT;
        let bit_index = number_in_bitmap % BITS_PER_BITMAP_UNIT;

        (bitmap_index, bit_index)
    }

	/// # Errors
	///
	/// Returns [`AllocError`] if no free memory was found.
    fn allocate_one(&mut self) -> Result<RawFrame, AllocError> {
	    let mut iter = self.bitmap.get_mut().iter_mut().enumerate();
	    let (i, first_set_bit) = loop {
		    let Some((i, entry)) = iter.next() else { alloc_err!(self, "No free memory"); };

		    let first_set_bit = entry.trailing_zeros() as usize;
		    if first_set_bit != BITS_PER_BITMAP_UNIT {
			    break (i, first_set_bit);
		    }
	    };

	    let bits_to_start = i * BITS_PER_BITMAP_UNIT;
	    let start = self.first_frame + bits_to_start + first_set_bit;
	    debug_assert_matches!(self.get_frame(start), Ok(FrameState::Free));
	    self.bitmap.get_mut()[i] &= !(1usize << first_set_bit);
	    debug_assert_matches!(self.get_frame(start), Ok(FrameState::Allocated));
	    Ok(start)
    }

	/// # Errors
	///
	/// Returns [`AllocError`] if no free memory was found.
    #[cold]
    fn allocate_multiple_slow(&mut self, frame_count: NonZero<usize>) -> Result<RawFrame, AllocError> {
        debug_assert!(frame_count.get() > 1, "`allocate_one` is faster for single pages");

        // TODO: Can this be sped up? I hope so

        let mut found_contiguous_frames = 0;
        let mut contiguous_frames_start = Option::<(usize, usize)>::None;
        let mut found = false;

        'outer: for (word_idx, entry) in self.bitmap.get_mut().iter_mut().enumerate() {
            for bit_idx in 0..BITS_PER_BITMAP_UNIT {
                let free = ((*entry >> bit_idx) & 1) == 1;
                if free {
                    if found_contiguous_frames == 0 {
                        contiguous_frames_start = Some((word_idx, bit_idx));
                    }
                    found_contiguous_frames += 1;
                    if found_contiguous_frames == frame_count.get() {
                        found = true;
                        break 'outer;
                    }
                }
                else {
                    found_contiguous_frames = 0;
                    contiguous_frames_start = None;
                }
            }
        }

        if found {
            let contiguous_frames_start = contiguous_frames_start.unwrap_or_else(|| unreachable!("found a free frame in Narnia"));
            let start = self.first_frame + (contiguous_frames_start.0 * BITS_PER_BITMAP_UNIT) + contiguous_frames_start.1;

            for frame in start..(start + frame_count.get()) {
	            debug_assert_matches!(self.get_frame(frame), Ok(FrameState::Free), "allocated an already allocated frame");
                self.set_frame(frame, FrameState::Allocated)
                        .unwrap_or_else(|_| unreachable!("cannot have allocated an out of range frame"));
            }

            Ok(start)
        } else {
		    alloc_err!(self, "No free memory");
	    }
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct BitmapAllocator(Spinlock<BitmapAllocatorInner>);

// SAFETY: Pmm<true> invaraints upheld by caller of `BitmapAllocator::new`
unsafe impl Pmm<true> for BitmapAllocator {
    fn allocate_raw(&self, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
        let mut guard = self.0.lock();

        let alloc = if count.get() == 1 { guard.allocate_one()? }
        else {
            guard.allocate_multiple_slow(count)?
            /*guard.allocate_multiple_fast(count)
                    .or_else(|_| guard.allocate_multiple_slow(count))?*/
        };

        debug!(target: "allocsan", "=== bmp a {:#018x} -> {:#018x}", alloc, alloc + count.get());

        Ok(alloc)
    }

	fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
        let mut guard = self.0.lock();

        let end = at + count.get();
		let free = (at..end).all(|f| guard.get_frame(f).is_ok_and(|state| state == FrameState::Free));
        if !free { alloc_err!(self, "Requested memory at {:x?} already allocated", at); }
        (at..end).for_each(|f| guard.set_frame(f, FrameState::Allocated).expect("Must be in range"));

		debug!(target: "allocsan", "=== bmp a {:#018x} -> {:#018x}", at, at + count.get());

        Ok(at)
    }

    /*fn push(&mut self, allocation: AllocationMeta) {
        let allocator = self.0.get_mut();

        for frame in allocation.region {
            let _ = allocator.set_frame(frame, FrameState::Allocated);
            //trace!(target: "allocsan", "=== bmp a {:#018x} -> {:#018x}", frame.start().addr, (frame + 1usize).start().addr);
        }
    }*/

	unsafe fn deallocate_raw(&self, base: RawFrame, count: NonZero<usize>) {
	    let mut guard = self.0.lock();

	    for i in 0..count.get() {
	        let frame = base + i;
	        guard.set_frame(frame, FrameState::Free)
	                .expect("Attempted to free frame that wasn't allocated by this allocator");
	    }

	    debug!(target: "allocsan", "=== bmp d {:#018x} -> {:#018x}", base, base + count.get());
	}
}

impl BitmapAllocator {
    /// # Safety
    ///
    /// The intersection of frames in `allocation_range` and `regions` must be uniquely owned by
    /// this allocator.
    /// All frames in the intersection of `allocation_range` and `regions` must be standard memory
    /// and not aliased by any hardware, such as memory for MMIO registers is.
    ///
    /// # Errors
    ///
    /// Returns [`AllocError`] on memory allocation failure for the bitmap.
    ///
    /// # Panics
    ///
    /// Panics if `allocation_range` is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use bitmap_allocator::BitmapAllocator;
    /// use kernel_api::memory::{RawFrame, PAGE_SIZE};
    ///
    /// let allocator = unsafe {
    ///     BitmapAllocator::new(
    ///         RawFrame::new(0)..RawFrame::new(PAGE_SIZE * 16),
    ///         [RawFrame::new(0)..RawFrame::new(PAGE_SIZE * 16)].into_iter(),
    ///     ).unwrap()
    /// };
    /// ```
    pub unsafe fn new(allocation_range: Range<RawFrame>, free_regions: impl Iterator<Item = Range<RawFrame>>) -> Result<&'static Self, AllocError> {
        let Range { start, end } = allocation_range;
        let mut inner = BitmapAllocatorInner::new(start, NonZero::new(end - start).unwrap())?;

        for free_region in free_regions {
            for frame in free_region {
	            if inner.bitmap.as_frame_range().contains(&frame) {
		            debug!("frame {frame:x?} in use");
	            } else {
		            inner.set_frame(frame, FrameState::Free)
		                     .unwrap_or_else(|_| panic!("Free frame outside of bitmap {frame:x?}"));
	            }
            }
        }

	    let this = Self(Spinlock::new(inner));
	    let pages_needed = NonZero::new(size_of::<Self>().div_ceil(PAGE_SIZE))
			    .expect("`div_ceil` with non-zero LHS should return non-zero result");

	    // FIXME: no drop impl means this memory gets leaked
	    let frame = this.allocate_raw(pages_needed)?;
	    #[expect(clippy::cast_ptr_alignment, reason = "checked by const assertion")]
	    let ptr = frame.to_virtual().as_ptr().cast::<MaybeUninit<Self>>();

	    const { assert!(align_of::<Self>() <= PAGE_SIZE, "page aligned too low to hold BitmapAllocator"); }

	    // SAFETY: BitmapAllocator only allocates ram that exists in page map, and alignment and size
	    //  are valid for `Self`
	    let ptr = unsafe { &mut *ptr };
	    Ok(ptr.write(this))
    }
}
