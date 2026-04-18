#![cfg_attr(not(test), no_std)]

#![deny(unsafe_code)]
#![deny(warnings)]

use core::debug_assert_matches;
use core::num::NonZero;
use core::ops::Range;
use kernel_api::memory::{RawFrame, Frames, PAGE_SIZE};
use kernel_api::sync::Spinlock;
use log::{debug, trace};
use kernel_api::allocator::{highmem, Pmm, AllocError};
use kernel_api::dbg;

const BITS_PER_BITMAP_UNIT: usize = size_of::<usize>() * 8;

macro_rules! alloc_err {
    ($reason:literal) => {
        debug!(concat!("BitmapAllocator: ", $reason));
        return Err(AllocError::pmm().into());
    };

    ($reason:literal, $($arg:tt)+) => {
        debug!(concat!("BitmapAllocator: ", $reason), $($arg)+);
        return Err(AllocError::pmm().into());
    };
}

#[derive(Debug, Eq, PartialEq)]
enum FrameState {
    Allocated,
    Free
}

#[derive(Debug)]
struct OutOfRangeError;

struct BitmapAllocator {
    first_frame: RawFrame,
    bitmap: Frames<true, usize>,
}

impl BitmapAllocator {
    fn last_frame(&self) -> RawFrame {
        self.first_frame + self.bitmap.get().len() * BITS_PER_BITMAP_UNIT
    }

    fn set_frame(&mut self, frame: RawFrame, state: FrameState) -> Result<(), OutOfRangeError> {
        if (frame < self.first_frame) || (frame >= self.last_frame()) { return Err(OutOfRangeError); }

        let (bitmap_index, bit_index) = self.frame_to_indices(frame);

	    debug_assert_ne!(self.get_frame(frame)?, state);

        match state {
            FrameState::Allocated => self.bitmap.get_mut()[bitmap_index] &= !(1 << bit_index),
            FrameState::Free => self.bitmap.get_mut()[bitmap_index] |= 1 << bit_index,
        }

        Ok(())
    }

    fn get_frame(&self, frame: RawFrame) -> Result<FrameState, OutOfRangeError> {
        if (frame < self.first_frame) || (frame >= self.last_frame()) { return Err(OutOfRangeError); }

        let (bitmap_index, bit_index) = self.frame_to_indices(frame);
        if (self.bitmap.get()[bitmap_index] & (1 << bit_index)) == 0 { Ok(FrameState::Allocated) }
        else { Ok(FrameState::Free) }
    }

    fn new(first_frame: RawFrame, frame_count: NonZero<usize>) -> Result<Self, AllocError> {
        let bitmap_length = frame_count.div_ceil(
            const { NonZero::new(8 * PAGE_SIZE).unwrap() }
        );
	    let _ = dbg!(first_frame, frame_count, bitmap_length);
        let bitmap = highmem().allocate(bitmap_length)?;
        let bitmap = bitmap.cast::<usize>().into_filed(0);

        Ok(Self {
            first_frame,
            bitmap
        })
    }

    fn frame_to_indices(&self, frame: RawFrame) -> (usize, usize) {
        assert!(frame >= self.first_frame);

        let number_in_bitmap = frame - self.first_frame;
        let bitmap_index = number_in_bitmap / BITS_PER_BITMAP_UNIT;
        let bit_index = number_in_bitmap % BITS_PER_BITMAP_UNIT;

        (bitmap_index, bit_index)
    }

    fn allocate_one(&mut self) -> Result<RawFrame, AllocError> {
	    let mut iter = self.bitmap.get_mut().iter_mut().enumerate();
	    let (i, first_set_bit) = loop {
		    let Some((i, entry)) = iter.next() else { alloc_err!("No free memory"); };

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

    #[expect(unused)]
    fn allocate_multiple_fast(&mut self, frame_count: usize) -> Result<RawFrame, AllocError> {
        assert!(frame_count > 1);

        // Cannot allocate bigger than number of bits in usize since can't check across boundaries
        if frame_count > BITS_PER_BITMAP_UNIT { alloc_err!("Too many pages for allocate_multiple_fast"); }

        // Create a mask of `frame_count` contiguous bits
        let mask = if frame_count == BITS_PER_BITMAP_UNIT { usize::MAX }
                          else { (1 << frame_count) - 1 };

        for (i, entry) in self.bitmap.get_mut().iter_mut().enumerate() {
            // locate the first free frame so we don't waste time checking unnecessary bits
            let first_set_bit = usize::try_from(entry.trailing_zeros()).unwrap();

            /*
             We slide the entry along, masking off the number of frames we want, and checking all the frames are free
             We can't slide further than `size_of(usize) - frame_count` as this would mean there can't possibly be enough frames left,
             since less than `frame_count` bits came from the original entry

             `frame_count = 2`
             `mask = 0b00000011`
             `entry = 0b01100100`

             `first_set_bit` is `2`, so we start with `slide = 2`
             `shifted = entry >> slide = 0b00011001`
             Then we mask the entry
             `masked = shifted & mask = 0b00000001`
             If enough frames were free, then the masked result should be equal to the mask, and we can allocate
             If not, we repeat with a larger slide
             */
            for slide in first_set_bit..(BITS_PER_BITMAP_UNIT - frame_count) {
                let shifted = *entry >> slide;
                let masked = shifted & mask;
                if masked == mask {
                    *entry &= !(mask << slide);
                    let bits_to_start = i * BITS_PER_BITMAP_UNIT;
                    let start = self.first_frame + bits_to_start + slide;
                    return Ok(start);
                }
            }
        }

        alloc_err!("No free memory");
    }

    #[cold]
    fn allocate_multiple_slow(&mut self, frame_count: NonZero<usize>) -> Result<RawFrame, AllocError> {
        assert!(frame_count.get() > 1);

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

        if !found { alloc_err!("No free memory"); }
        else {
            let contiguous_frames_start = contiguous_frames_start.expect("unreachable");
            let start = self.first_frame + (contiguous_frames_start.0 * BITS_PER_BITMAP_UNIT) + contiguous_frames_start.1;

            for frame in start..(start + frame_count.get()) {
	            debug_assert_matches!(self.get_frame(frame), Ok(FrameState::Free));
                self.set_frame(frame, FrameState::Allocated)
                        .expect("Cannot have allocated an out of range frame");
            }

            Ok(start)
        }
    }
}

pub struct Wrapped(Spinlock<BitmapAllocator>);

#[allow(unsafe_code)]
unsafe impl Pmm<true> for Wrapped {
    fn allocate_raw(&self, frame_count: NonZero<usize>) -> Result<RawFrame, AllocError> {
        let mut guard = self.0.lock();

        let alloc = if frame_count.get() == 1 { guard.allocate_one()? }
        else {
            guard.allocate_multiple_slow(frame_count)?
            /*guard.allocate_multiple_fast(frame_count)
                    .or_else(|_| guard.allocate_multiple_slow(frame_count))?*/
        };

        trace!("=== bmp a {:#018x} -> {:#018x}", alloc, alloc + frame_count.get());

        Ok(alloc)
    }

	fn allocate_raw_at(&self, at: RawFrame, count: NonZero<usize>) -> Result<RawFrame, AllocError> {
        let mut guard = self.0.lock();

        let end = at + count.get();
        let free = (at..end).all(|f| match guard.get_frame(f) {
            Ok(state) => state == FrameState::Free,
            Err(_) => false,
        });
        if !free { alloc_err!("Requested memory at {:x?} already allocated", at); }
        (at..end).for_each(|f| guard.set_frame(f, FrameState::Allocated).expect("Must be in range"));

        trace!("=== bmp a {:#018x} -> {:#018x}", at, at + count.get());

        Ok(at)
    }

    /*fn push(&mut self, allocation: AllocationMeta) {
        let allocator = self.0.get_mut();

        for frame in allocation.region {
            let _ = allocator.set_frame(frame, FrameState::Allocated);
            //trace!("=== bmp a {:#018x} -> {:#018x}", frame.start().addr, (frame + 1usize).start().addr);
        }
    }*/

	unsafe fn deallocate_raw(&self, base: RawFrame, frame_count: NonZero<usize>) {
	    let mut guard = self.0.lock();

	    for i in 0..frame_count.get() {
	        let frame = base + i;
	        guard.set_frame(frame, FrameState::Free)
	                .expect("Attempted to free frame that wasn't allocated by this allocator");
	    }

	    trace!("=== bmp d {:#018x} -> {:#018x}", base, base + frame_count.get());
	}
}

impl Wrapped {
	#[allow(unsafe_code)]
    pub unsafe fn new(allocation_range: Range<RawFrame>, regions: impl Iterator<Item = Range<RawFrame>>) -> Result<&'static Self, AllocError> {
        let Range { start, end } = dbg!(allocation_range);
        let mut allocator = BitmapAllocator::new(start,  NonZero::new(end - start).unwrap())?;
		let _ = dbg!(allocator.first_frame, allocator.last_frame());

        for free_region in regions {
            for frame in free_region {
	            if !allocator.bitmap.as_frame_range().contains(&frame) {
		            allocator.set_frame(frame, FrameState::Free)
		                     .unwrap_or_else(|_| panic!("Free frame outside of bitmap {frame:x?}"));
	            } else {
		            debug!("frame {frame:x?} in use");
	            }
            }
        }

		const {
			assert!(size_of::<Wrapped>() <= PAGE_SIZE);
			assert!(align_of::<Wrapped>() <= PAGE_SIZE);
		};
		let allocator_frame = allocator.allocate_one()?;
		trace!("=== bmp a {:#018x} -> {:#018x}", allocator_frame, allocator_frame + 1usize);
		let allocator_ptr = allocator_frame.to_virtual().as_ptr().cast::<Wrapped>();
		unsafe {
			allocator_ptr.write(Wrapped(Spinlock::new(allocator)));
			Ok(&*allocator_ptr)
		}
    }
}

#[cfg(test)]
mod tests {}
