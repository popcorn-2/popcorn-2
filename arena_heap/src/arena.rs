use core::alloc::Layout;
use core::num::NonZero;
use core::ops::Range;
use core::ptr::NonNull;
use log::debug;
use kernel_api::dbg;
use kernel_api::memory::AllocError;
use kernel_api::memory::mapping::{Mapping, Config, new_mapping};
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
			(capacity + 2*size_of::<ChunkHeader>()).div_ceil((4096 * 2) / 3), // add a bit of extra space
		);

		let mapping = new_mapping(
			Config::new(NonZero::new(page_count).unwrap()),
			25
		)?;
		
		let start = mapping.virtual_valid_start().as_ptr().cast::<ChunkHeader>();
		let end = unsafe { mapping.virtual_valid_end().as_ptr().cast::<ChunkHeader>().offset(-1) };

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
		let ptr = self.mapping.virtual_valid_start().as_ptr().cast::<ChunkHeader>();
		NonNull::new(ptr).expect("arena mapping should not be null")
	}

	fn alloc_in_aligned(chunk: &mut ChunkHeader, layout: Layout) -> Result<NonNull<u8>, AllocError> {
		let size = layout.size();
		let align = layout.align();

		assert!(chunk.size() >= size);
		assert!(!chunk.busy());

		let start_pointer = chunk.start();
		let end_pointer = chunk.end();

		assert_eq!(start_pointer.align_offset(align), 0);

		// split the allocation if possible (defined as having leftover space big enough for
		// `MINIMUM_USABLE_ALLOC` plus an aligned header

		// SAFETY: the chunk is large enough to contain an allocation of `size`, so `start + size` is either
		// within the chunk, or one past the end
		let split_point = unsafe { start_pointer.byte_add(size) };

		// do all these calculations in terms of usize to ensure we never go out of bounds of the allocation
		let new_header = split_point.addr()
		                            .checked_add(split_point.align_offset(align_of::<ChunkHeader>()));
		let new_end = new_header
				.map(|val| val.checked_add(size_of::<ChunkHeader>() + Self::MINIMUM_USABLE_ALLOC))
				.flatten();

		if let Some(new_end) = new_end && new_end < end_pointer.addr() {
			let new_header = new_header.unwrap();
			debug!("insert new chunk at {new_header:#x}");

			// provenance of `start_pointer` covers the entire chunk and we just checked that `new_end..(new_end + aligned<ChunkHeader>)`
			// is within the chunk
			let new_header = start_pointer.with_addr(new_header);

			let new_chunk = unsafe { ChunkHeader::new(
				chunk.next(),
				Some(NonNull::from(&mut *chunk))
			) };

			unsafe { new_header.cast().write(new_chunk); }

			unsafe {
				chunk.next().expect("cannot be allocating sentinel chunk")
				     .as_mut()
				     .set_prev(Some(new_header.cast()));
			}

			chunk.set_next(Some(new_header.cast()));

		}

		chunk.set_busy(true);

		// provenance-exposition: reduce bounds on this to only cover `start_pointer..end_pointer`
		return Ok(start_pointer);
	}

	fn alloc_in(chunk: &mut ChunkHeader, layout: Layout) -> Result<NonNull<u8>, AllocError> {
		let size = layout.size();
		let align = layout.align();

		assert!(chunk.size() >= size);
		assert!(!chunk.busy());

		let start_pointer = chunk.start();
		let end_pointer = chunk.end();

		let align_offset = start_pointer.align_offset(align);
		if chunk.size() < (size + align_offset) { return Err(AllocError); }

		if align_offset == 0 {
			Self::alloc_in_aligned(chunk, layout)
		} else {// SAFETY: the chunk is large enough to contain an allocation of `size + align_offset`, so `start + align_offset`
			// must be within the chunk
			let split_point = unsafe { start_pointer.add(align_offset) };
			let mut new_chunk_ptr = unsafe { split_point.cast::<ChunkHeader>().offset(-1) };
			
			if align_offset > size_of::<ChunkHeader>() + Self::MINIMUM_USABLE_ALLOC {
				debug!("insert new chunk at {new_chunk_ptr:p}");

				let new_chunk = unsafe { ChunkHeader::new(
					chunk.next(),
					Some(NonNull::from(&mut *chunk))
				) };

				unsafe { new_chunk_ptr.cast().write(new_chunk); }

				unsafe {
					chunk.next().expect("cannot be allocating sentinel chunk")
					     .as_mut()
					     .set_prev(Some(new_chunk_ptr.cast()));
				}

				chunk.set_next(Some(new_chunk_ptr.cast()));
			} else {
				debug!("expand previous alloc");
				
				let Some(mut prev) = chunk.prev() else {
					debug!("no previous alloc to expand");
					return Err(AllocError);
				};
				
				let chunk_header = chunk.clone();
				let prev = unsafe { prev.as_mut() };
				prev.set_next(Some(new_chunk_ptr));
				if let Some(mut next) = chunk.next() {
					unsafe { next.as_mut() }.set_prev(Some(new_chunk_ptr));
				}
				unsafe { new_chunk_ptr.write(chunk_header) };
			}
			
			Self::alloc_in_aligned(unsafe { new_chunk_ptr.as_mut() }, layout)
		}
	}

	pub fn try_alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
		let size = layout.size();

		for chunk in &mut *self {
			if chunk.size() < size { continue; }
			if chunk.busy() { continue; }

			if let Ok(res) = Self::alloc_in(chunk, layout) { return Ok(res); }
		}

		// if no space in the existing chunks,
		let min_expansion = self.mapping.physical_len().checked_mul(NonZero::new(Self::GROW_FACTOR - 1).unwrap()).ok_or(AllocError)?;
		let alloc_expansion = (size + 2*size_of::<ChunkHeader>()).div_ceil((4096 * 2) / 3);
		let expansion = core::cmp::max(alloc_expansion, min_expansion.get());

		let alloc_start = self.mapping.virtual_valid_end().as_ptr();
		let old_sentinel = unsafe { &mut *alloc_start.cast::<ChunkHeader>().offset(-1) };

		dbg!(expansion, min_expansion, alloc_expansion);

		dbg!(self.mapping.resize_in_place(self.mapping.physical_len().checked_add(expansion).ok_or(AllocError)?))?;

		let new_sentinel = unsafe { self.mapping.virtual_valid_end().as_ptr().cast::<ChunkHeader>().offset(-1) };

		unsafe {
			new_sentinel.write(ChunkHeader::new(
				None,
				Some(NonNull::from(&mut *old_sentinel)),
			));
		}
		old_sentinel.set_next(Some(NonNull::new(new_sentinel).expect("mmap should not end at null")));

		Self::alloc_in(old_sentinel, layout)
	}

	pub fn bounds(&self) -> Range<NonNull<u8>> {
		Range {
			start: NonNull::new(self.mapping.virtual_valid_start().as_ptr()).expect("arena should not be at null"),
			end: NonNull::new(self.mapping.virtual_valid_end().as_ptr()).expect("arena should not be at null"),
		}
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
