use core::alloc::Layout;
use core::num::NonZero;
use core::ptr::NonNull;
use log::debug;
use kernel_api::memory::AllocError;
use kernel_api::memory::mapping::{Mapping, Config};
use crate::chunk::ChunkHeader;

pub struct Arena {
	mapping: Mapping<'static>,
}

impl Arena {
	const INITIAL_AREA_PAGE_COUNT: usize = 4; // 16 KiB
	const GROW_FACTOR: usize = 2;
	const MINIMUM_USABLE_ALLOC: usize = size_of::<usize>();
	
	pub fn with_capacity(capacity: usize) -> Result<Self, AllocError> {
		let page_count = core::cmp::max(
			Self::INITIAL_AREA_PAGE_COUNT,
			(capacity + 2*size_of::<ChunkHeader>()).div_ceil(4096),
		);
		
		let mapping = Mapping::new(
			Config::new(NonZero::new(page_count).unwrap()),
			25
		)?;
		
		let start = mapping.virtual_start().as_ptr().cast::<ChunkHeader>();
		let end = unsafe { mapping.virtual_end().as_ptr().byte_sub(size_of::<ChunkHeader>()) }.cast::<ChunkHeader>();

		// SAFETY: Pointer returned by Mapping::new is guaranteed to be valid for RW access for 16 KiB
		// start - pointer returned by Mapping::new is 4K aligned which is greater than `align_of::<ChunkHeader>()`
		// end - end of mapping is 4K aligned, therefore `virtual_end()` is more aligned than
		// `align_of::<ChunkHeader>()`, therefore subtracting `size_of::<ChunkHeader>()` will also be suitably aligned
		unsafe {
			start.write(
				ChunkHeader::new(
					Some(NonNull::new(end).expect("mmap should not return null ptr")),
					None,
				)
			);
			end.write(
				ChunkHeader::new(
					None,
					Some(NonNull::new(start).expect("mmap should not return null ptr")),
				)
			)
		}

		Ok(Self { mapping })
	}

	fn first_chunk(&self) -> NonNull<ChunkHeader> {
		let ptr = self.mapping.virtual_start().as_ptr().cast::<ChunkHeader>();
		NonNull::new(ptr).expect("arena mapping should not be null")
	}

	pub fn try_alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
		let size = layout.size();
		let align = layout.align();

		for chunk in self {
			if chunk.size() < size { continue; }
			if chunk.busy() { continue; }

			let start_pointer = chunk.start();
			let end_pointer = chunk.end();

			let align_offset = start_pointer.align_offset(align);

			if align_offset == 0 {
				// split the allocation if possible (defined as having leftover space big enough for
				// `MINIMUM_USABLE_ALLOC` plus an aligned header
				
				// SAFETY: the chunk is large enough to contain an allocation of `size`, so `start + size` is either
				// within the chunk, or one past the end
				let split_point = unsafe { start_pointer.byte_add(size) };
				
				// do all these calculations in terms of usize to ensure we never go out of bounds of the allocation
				let new_end = split_point.addr()
						.checked_add(split_point.align_offset(align_of::<ChunkHeader>()))
						.map(|val| val.checked_add(size_of::<ChunkHeader>() + Self::MINIMUM_USABLE_ALLOC))
						.flatten();
				
				if let Some(new_end) = new_end && new_end < end_pointer.addr() {
					debug!("insert new chunk at {new_end:#x}");
					
					// provenance of `start_pointer` covers the entire chunk and we just checked that `new_end..(new_end + aligned<ChunkHeader>)`
					// is within the chunk
					let new_end = start_pointer.with_addr(new_end);
					
					let new_chunk = unsafe { ChunkHeader::new(
						chunk.next(),
						Some(NonNull::from(&mut *chunk))
					) };
					
					unsafe { new_end.cast().write(new_chunk); }
					
					unsafe {
						chunk.next().expect("cannot be allocating sentinel chunk")
						     .as_mut()
						     .set_prev(Some(new_end.cast()));
					}
					
					chunk.set_next(Some(new_end.cast()));
					
				}

				chunk.set_busy(true);
				
				// provenance-exposition: reduce bounds on this to only cover `start_pointer..end_pointer`
				return Ok(start_pointer);
			} else {
				todo!("handle alignment")
			}
		}

		Err(AllocError)
	}
}

impl<'arena> IntoIterator for &'arena mut Arena {
	type Item = &'arena mut ChunkHeader;
	type IntoIter = IterMut<'arena>;

	fn into_iter(self) -> Self::IntoIter {
		IterMut {
			// SAFETY: See notes for `IterMut::next()`
			chunk: Some(unsafe { self.first_chunk().as_mut() })
		}
	}
}

pub struct IterMut<'arena> {
	chunk: Option<&'arena mut ChunkHeader>,
}

impl<'arena> Iterator for IterMut<'arena> {
	type Item = &'arena mut ChunkHeader;

	fn next(&mut self) -> Option<Self::Item> {
		let current = self.chunk.take()?;
		
		// SAFETY: Arena is uniquely borrowed, so no one can have a pointer to the chunk header
		// via the arena, and all other references to chunk headers (within the linked list) are
		// through raw pointers which are valid to alias as long as no access occurs through them
		// (which it can't due to the arena being uniquely borrowed)
		let next = unsafe { current.next().map(|mut ptr| ptr.as_mut()) };
		match next {
			Some(next) => self.chunk = Some(next),
			None => return None, // if `current.next()` is None then we would be returning the sentinel chunk which we don't want to do
		}
		
		Some(current)
	}
}
