use core::alloc::Layout;
use core::num::NonZero;
use core::ops::Range;
use core::ptr::NonNull;
use log::trace;
#[cfg(not(test))] use kernel_api::address_space::Kernel;
use kernel_api::allocator::AllocError;
#[cfg(not(test))] use kernel_api::mapping::{Mapping, Config, Mmap, Ty};
use kernel_api::memory::asan::{asan_free_range, set_shadow_heap_free, set_shadow_heap_left, set_shadow_heap_header, set_shadow_heap_right, count_to_shadow, mem_to_shadow, no_asan_shim};
use kernel_api::memory::{PAGE_SIZE, VirtualAddress};

const GENERATION_FREE: usize = 0;
#[cfg(feature = "generations")] const GENERATION_ALLOCATED: usize = 8;
#[cfg(not(feature = "generations"))] const GENERATION_ALLOCATED: usize = 2;

#[cfg(test)]
mod mock {
	#![allow(unused)]

	use alloc::boxed::Box;
	use core::num::NonZero;
	use core::ops::Range;
	use kernel_api::allocator::AllocError;
	use kernel_api::memory::PAGE_SIZE;

	#[repr(C, align(4096))]
	struct Page([u8; 4096]);

	pub struct Mapping {
		backing: aliasable::boxed::AliasableBox<[Page]>,
	}

	impl Mapping {
		pub fn new(page_count: NonZero<usize>) -> Self {
			let backing = Box::new_zeroed_slice(page_count.get());
			let backing = unsafe { backing.assume_init() };
			Self {
				backing: backing.into(),
			}
		}

		pub fn as_mut_ptr_range(&mut self) -> Range<*mut u8> {
			let pages = self.backing.as_mut_ptr_range();
			Range {
				start: pages.start.cast(),
				end: pages.end.cast(),
			}
		}

		pub fn as_ptr_range(&self) -> Range<*const u8> {
			let pages = self.backing.as_ptr_range();
			Range {
				start: pages.start.cast(),
				end: pages.end.cast(),
			}
		}

		pub fn page_len(&self) -> usize { self.backing.len() }
		pub fn byte_len(&self) -> usize { self.backing.len() * 4096 }

		pub fn as_ptr(&self) -> *const u8 { self.as_ptr_range().start }
		pub fn as_mut_ptr(&mut self) -> *mut u8 { self.as_mut_ptr_range().start }

		pub fn grow_in_place_by(&mut self, _extra_length: usize) -> Result<(), AllocError> {
			Err(AllocError::vmm())
		}
	}
}

pub struct Arena {
	#[cfg(not(test))] mapping: Mapping<Mmap, Kernel>,
	#[cfg(test)] mapping: mock::Mapping,
	first: *mut ChunkHeader,
	len: usize,
}

unsafe impl Send for Arena {}

#[repr(C)] // to make sure we can reuse the redzone just by offsetting -1 usize from end
pub struct ChunkHeader {
	next: *mut Self,
	prev: *mut Self,
	generation: usize,
	checksum: isize,
	magic: usize,
	self_offset: usize,
}

impl ChunkHeader {
	const CHUNK_MAGIC: isize = isize::from_be_bytes(*b"POP HEAP");
	const ALIGN_OFFSET_MAGIC: usize = usize::from_be_bytes(*b"MAGICPOP");

	unsafe fn new_in(self: *mut Self, next: *mut Self, prev: *mut Self, generation: usize) {
		unsafe {
			self.update_in_place(next, prev, generation);
		}

		if !next.is_null() {
			debug_assert!(self.addr() < next.addr(), "`next` ({next:p}) must be after `self` ({self:p})");
		}
	}

	unsafe fn update_in_place(self: *mut Self, next: *mut Self, prev: *mut Self, generation: usize) {
		debug_assert!(self.addr() > prev.addr(), "`prev` ({prev:p}) must be before `self` ({self:p})");

		if !next.is_null() {
			debug_assert!(self.addr() < next.addr(), "`next` ({next:p}) must be after `self` ({self:p})");
		}

		let checksum = (next.addr() as isize) + (prev.addr() as isize);

		unsafe {
			(*self).next = next;
			(*self).prev = prev;
			(*self).generation = generation;
			(*self).checksum = Self::CHUNK_MAGIC - checksum;
		}
	}

	#[cfg_attr(kasan, sanitize(address = "off"))]
	#[cfg_attr(kasan, inline(never))]
	unsafe fn size(self: *mut Self) -> usize {
		assert!(unsafe { !(*self).next.is_null() }, "sentinel chunk has no size");
		let total_size = unsafe { (*self).next.byte_offset_from_unsigned(self) };
		total_size - size_of::<Self>()
	}

	const unsafe fn start_ptr(self: *mut Self) -> *mut u8 {
		unsafe { self.cast::<u8>().byte_add(size_of::<Self>()) }
	}

	#[cfg_attr(kasan, sanitize(address = "off"))]
	#[cfg_attr(kasan, inline(never))]
	unsafe fn check_magic(self: *mut Self) {
		let next = unsafe { (*self).next };
		let prev = unsafe { (*self).prev };

		let checksum = (next.addr() as isize) + (prev.addr() as isize);
		assert_eq!(
			unsafe { (*self).checksum } + checksum,
			Self::CHUNK_MAGIC,
			"heap header magic invalid"
		);

		debug_assert!(self.addr() > prev.addr(), "`prev` ({prev:p}) must be before `self` ({self:p})");

		if !next.is_null() {
			debug_assert!(self.addr() < next.addr(), "`next` ({next:p}) must be after `self` ({self:p})");
		}
	}
}

impl Arena {
	const INITIAL_AREA_PAGE_COUNT: usize = 4; // 16 KiB
	const GROW_FACTOR: usize = 2;
	const MINIMUM_USABLE_ALLOC: usize = 2 * size_of::<usize>();

	pub fn with_capacity(capacity: usize) -> Result<Self, AllocError> {
		let page_count = core::cmp::max(
			Self::INITIAL_AREA_PAGE_COUNT,
			(capacity + 2 * size_of::<ChunkHeader>()).div_ceil((4096 * 2) / 3), // add a bit of extra space
		);

		#[cfg(not(test))] let mut mapping = Config::new(NonZero::new(page_count).unwrap(), Ty::HEAP)
				.protection(true, false, false)
				.map()?;
		#[cfg(test)] let mut mapping = mock::Mapping::new(NonZero::new(page_count).unwrap());

		let Range { start, end } = mapping.as_mut_ptr_range();
		let start = start.cast::<ChunkHeader>();
		let end = unsafe { end.cast::<ChunkHeader>().sub(1) };

		let len = mapping.byte_len();

		// SAFETY: Pointer returned by Mapping::new is guaranteed to be valid for RW access for 16 KiB
		//  start - pointer returned by Mapping::new is 4K aligned which is greater than `align_of::<ChunkHeader>()`
		//  end - end of mapping is 4K aligned, therefore `virtual_end()` is more aligned than
		//  `align_of::<ChunkHeader>()`, therefore subtracting `size_of::<ChunkHeader>()` will also be suitably aligned
		unsafe {
			start.new_in(
				end,
				core::ptr::null_mut(),
				GENERATION_FREE,
			);
			(*start).next.new_in(
				core::ptr::null_mut(),
				start,
				usize::MAX,
			);
		}

		unsafe {
			set_shadow_heap_left(
				mem_to_shadow(start.into()),
				count_to_shadow(page_count * 4096)
			);
		}

		unsafe {
			set_shadow_heap_header(
				mem_to_shadow(start.into()),
				count_to_shadow(size_of::<ChunkHeader>())
			);
		}

		no_asan_shim!(|start: *mut ChunkHeader| {
			unsafe {
				set_shadow_heap_header(
					mem_to_shadow((*start).next.into()),
					count_to_shadow(size_of::<ChunkHeader>())
				);
			}
		});

		Ok(Self {
			len,
			first: start,
			mapping,
		})
	}

	pub fn try_alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
		let size = layout.size() + 64; // fixme: hacksssssss

		let mut current_chunk = self.first;
		loop {
			unsafe { current_chunk.check_magic() };

			let current_chunk = &mut current_chunk;
			let res = no_asan_shim!(|current_chunk: &mut *mut ChunkHeader, layout: Layout| -> Option<NonNull<u8>> {
				let size = layout.size() + 64; // fixme: hacksssssss

				let generation = unsafe { (**current_chunk).generation };
				if unsafe { (**current_chunk).next.is_null() } {
					trace!("check chunk at {:#p} with generation {generation:#x}", *current_chunk);
				} else {
					trace!("check chunk at {:#p} with generation {generation:#x}, len {}", *current_chunk, unsafe { current_chunk.size() });
				}
				match generation {
					GENERATION_FREE => {
						if unsafe { current_chunk.size() } >= size {
							trace!("found big enough chunk");
							if let Ok(res) = unsafe { Arena::alloc_in(*current_chunk, layout) } { return Some(res); }
						}
					}
					#[allow(unreachable_patterns, reason = "may not exist depending on how many generations exist")]
					GENERATION_FREE..GENERATION_ALLOCATED => {
						unsafe { (**current_chunk).generation -= 1 };
					}
					_ => {}
				}
				*current_chunk = unsafe { (**current_chunk).next };

				None
			});

			if let Some(res) = res { return Ok(res); }

			if current_chunk.is_null() { break; }
		}

		// if no space in the existing chunks,
		let grow_expansion = self.mapping.page_len().checked_mul(Self::GROW_FACTOR - 1).ok_or(AllocError::heap())?;
		let alloc_expansion = (size + 2*size_of::<ChunkHeader>()).div_ceil((PAGE_SIZE * 2) / 3);
		let max_expansion = core::cmp::min(grow_expansion, 128);
		let expansion = core::cmp::max(alloc_expansion, max_expansion);

		let alloc_end = self.mapping.as_mut_ptr_range().end;
		let old_sentinel = unsafe { alloc_end.cast::<ChunkHeader>().sub(1) };
		unsafe { old_sentinel.check_magic() };

		self.mapping.grow_in_place_by(expansion)?;

		let new_sentinel = unsafe { self.mapping.as_mut_ptr_range().end.cast::<ChunkHeader>().sub(1) };

		no_asan_shim!(|old_sentinel: *mut ChunkHeader, new_sentinel: *mut ChunkHeader| {
			unsafe {
				new_sentinel.new_in(
					core::ptr::null_mut(),
					old_sentinel,
					usize::MAX,
				);

				old_sentinel.new_in(
					new_sentinel,
					(*old_sentinel).prev,
					GENERATION_FREE,
				);
			}
		});

		self.len = self.mapping.byte_len();
		self.first = self.mapping.as_mut_ptr().cast::<ChunkHeader>();

		unsafe {
			set_shadow_heap_left(
				mem_to_shadow(old_sentinel.into()),
				count_to_shadow(expansion * 4096)
			);
		}

		unsafe {
			set_shadow_heap_header(
				mem_to_shadow(new_sentinel.into()),
				count_to_shadow(size_of::<ChunkHeader>())
			);
		}

		trace!("alloc at old sentinel = {old_sentinel:#p}");
		unsafe { Self::alloc_in(old_sentinel, layout) }
	}

	unsafe fn alloc_in(chunk: *mut ChunkHeader, layout: Layout) -> Result<NonNull<u8>, AllocError> {
		unsafe { chunk.check_magic() };

		let size = layout.size() + 64; // fixme: hackssssssss
		let align = layout.align();

		let start_ptr = unsafe { chunk.start_ptr() };
		let align_offset = start_ptr.align_offset(align);

		let chunk_size = unsafe { chunk.size() };
		if size + align_offset > chunk_size { return Err(AllocError::heap()); }

		#[cfg(not(kasan))] assert_eq!(unsafe { (*chunk).generation }, GENERATION_FREE);

		let (start, total_size) = unsafe {
			let start = start_ptr.add(align_offset);
			let end = start.add(size);
			let total_size = align_offset + size + end.align_offset(align_of::<ChunkHeader>());
			(start, total_size)
		};

		if total_size > chunk_size { return Err(AllocError::heap()); }
		else if chunk_size.saturating_sub(total_size + size_of::<ChunkHeader>()) > Self::MINIMUM_USABLE_ALLOC {
			no_asan_shim!(|chunk: *mut ChunkHeader, total_size: usize| {
				let new_header = unsafe { chunk.add(1).byte_add(total_size) };
				unsafe {
					// insert `new_header` between `chunk` and `chunk->next`
					new_header.new_in(
						(*chunk).next,
						chunk,
						GENERATION_FREE,
					);

					// update `chunk->next` to point to `new_header`
					(*chunk).next.update_in_place(
						(*(*chunk).next).next,
						new_header,
						(*(*chunk).next).generation,
					);

					// update `chunk` to point to `new_header`
					chunk.update_in_place(
						new_header,
						(*chunk).prev,
						(*chunk).generation,
					);
				}

				unsafe {
					set_shadow_heap_header(
						mem_to_shadow(new_header.into()),
						count_to_shadow(size_of::<ChunkHeader>())
					);
				}
			});
		}

		no_asan_shim!(|chunk: *mut ChunkHeader, start: *mut u8, align_offset: usize| {
			unsafe { (*chunk).generation = GENERATION_ALLOCATED; };
			assert_ne!(align_offset, usize::MAX, "invalid offset");
			unsafe { *start.cast::<usize>().sub(1) = align_offset };
			unsafe { *start.cast::<usize>().sub(2) = ChunkHeader::ALIGN_OFFSET_MAGIC };
		});

		unsafe {
			set_shadow_heap_left(
				mem_to_shadow(start_ptr.into()),
				count_to_shadow(start.byte_offset_from_unsigned(start_ptr)),
			);
		}

		let allocation_end = VirtualAddress::from(start) + layout.size();
		unsafe {
			set_shadow_heap_right(
				mem_to_shadow(allocation_end),
				count_to_shadow(64),
			);
		}

		asan_free_range(
			start.into(),
			layout.size(),
		);

		no_asan_shim!(
			|chunk: *mut ChunkHeader, start: *mut u8, layout: Layout| {
				debug_assert!(unsafe { (*chunk).next.byte_offset_from_unsigned(start) } >= layout.size());
			}
		);
		debug_assert!(allocation_end.addr - start.addr() >= layout.size());
		debug_assert!(start.is_aligned_to(layout.align()));

		unsafe { Ok(NonNull::new_unchecked(start)) }
	}

	pub const fn bounds(&self) -> Range<NonNull<u8>> {
		let end = unsafe { self.first.byte_add(self.len) };
		Range {
			start: NonNull::new(self.first.cast()).expect("arena should not be at null"),
			end: NonNull::new(end.cast()).expect("arena should not be at null"),
		}
	}

	pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>) {
		let ptr = self.first.with_addr(ptr.addr().get());

		let (offset, magic) = no_asan_shim!(|ptr: *mut ChunkHeader| -> (usize, usize) {
			let offset = unsafe { *ptr.cast::<usize>().offset(-1) };
			let magic = unsafe { *ptr.cast::<usize>().offset(-2) };
			(offset, magic)
		});
		if offset == usize::MAX { let _ = unsafe { *ptr.cast::<usize>().offset(-1) }; }
		debug_assert_eq!(magic, ChunkHeader::ALIGN_OFFSET_MAGIC, "align offset corrupted");
		assert_ne!(offset, usize::MAX, "invalid offset");
		let ptr = unsafe { ptr.byte_sub(offset).byte_sub(size_of::<ChunkHeader>()) };
		trace!("dealloc chunk from {ptr:#p} with offset {offset}");

		unsafe { ptr.check_magic() };

		no_asan_shim!(|ptr: *mut ChunkHeader| {
			assert_eq!(unsafe { (*ptr).generation }, GENERATION_ALLOCATED, "double free detected");
			if unsafe { ptr.size() } >= PAGE_SIZE {
				unsafe { (*ptr).generation = GENERATION_FREE };
			} else {
				unsafe { (*ptr).generation = GENERATION_ALLOCATED - 1 };
			}

			trace!("sanity: {:#p}, {:#p}", unsafe { (*ptr).prev }, unsafe { (*ptr).next });
		});

		unsafe {
			set_shadow_heap_free(
				mem_to_shadow(ptr.add(1).into()),
				count_to_shadow(ptr.size()),
			);
		}

		no_asan_shim!(|ptr: *mut ChunkHeader| {
			unsafe { (*ptr).next.check_magic() };
			if unsafe { (*(*ptr).next).generation } < GENERATION_ALLOCATED && unsafe { !(*(*ptr).next).next.is_null() } {
				trace!("merge right with {:#p}, next-of-next={:#p}", unsafe { (*ptr).next }, unsafe { (*(*ptr).next).next });

				unsafe {
					// update `ptr` to point to `ptr->next->next`
					ptr.update_in_place(
						(*(*ptr).next).next,
						(*ptr).prev,
						core::cmp::min((*ptr).generation, (*(*ptr).next).generation),
					);

					// update the new `ptr->next` (originally `ptr->next->next`) to point to `ptr`
					(*ptr).next.update_in_place(
						(*(*ptr).next).next,
						ptr,
						(*(*ptr).next).generation,
					);
				}
			}

			if unsafe { !(*ptr).prev.is_null() } && unsafe { (*(*ptr).prev).generation } < GENERATION_ALLOCATED {
				unsafe { (*ptr).prev.check_magic() };
				trace!("merge left");

				unsafe {
					// update `ptr->prev` to point to `ptr->next`
					(*ptr).prev.update_in_place(
						(*ptr).next,
						(*(*ptr).prev).prev,
						core::cmp::min((*ptr).generation, (*(*ptr).prev).generation)
					);

					// update `ptr->next` to point to `ptr->prev`
					(*ptr).next.update_in_place(
						(*(*ptr).next).next,
						(*ptr).prev,
						(*(*ptr).next).generation,
					);
				}
			}
		});
	}
}
