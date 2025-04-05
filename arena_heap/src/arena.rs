use core::num::NonZero;
use core::ptr::{addr_of, NonNull};
use kernel_api::memory::AllocError;
use kernel_api::memory::mapping::{Mapping, Config};
use crate::chunk::ChunkHeader;

pub struct Arena {
	mapping: Mapping<'static>,
}

impl Arena {
	const INITIAL_AREA_PAGE_COUNT: usize = 4; // 16 KiB
	const GROW_FACTOR: usize = 2;
	
	pub fn new() -> Result<Self, AllocError> {
		let mapping = Mapping::new(
			Config::new(NonZero::new(Self::INITIAL_AREA_PAGE_COUNT).unwrap()),
			25
		)?;
		
		let start = mapping.virtual_start().as_ptr().cast::<ChunkHeader>();
		let end = unsafe { mapping.virtual_end().as_ptr().byte_sub(size_of::<ChunkHeader>()) }.cast::<ChunkHeader>();
		
		// SAFETY: Pointer returned by Mapping::new is guaranteed to be valid for RW access for 16 KiB
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
	
	pub fn first_chunk(&self) -> NonNull<ChunkHeader> {
		let ptr = self.mapping.virtual_start().as_ptr().cast::<ChunkHeader>();
		NonNull::new(ptr).expect("arena mapping should not be null")
	}
}
