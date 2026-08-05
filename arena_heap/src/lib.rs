//! A heap allocator which allocates multiple arenas from the backing allocator to divide
//! into individual allocations.
//!
//! On a call to `allocate`, the free holes in each arena are searched for a suitable space.
//! If there are no suitable holes, then an arena will attempt to increase in size, or a new
//! arena will be allocated.
#![no_std]

#![feature(allocator_api)]
#![feature(pointer_is_aligned_to)]
#![feature(arbitrary_self_types_pointers)]
#![feature(strict_provenance_lints)]
#![cfg_attr(doc, feature(rustdoc_missing_doc_code_examples))]
#![cfg_attr(kasan, feature(sanitize))]

use core::alloc::{AllocError, Layout};
use core::ptr::NonNull;
use log::{debug, trace};
use arena::Arena;
use chunk::ChunkHeader;
use kernel_api::dbg;
use kernel_api::sync::{RwSpinlock, Spinlock};
#[cfg(not(test))] use crate::mapped_vec::MappedVec;
#[cfg(test)] extern crate alloc;
#[cfg(test)] use alloc::vec::Vec;

mod mapped_vec;
mod arena;

#[doc(hidden)]
#[unsafe(no_mangle)]
pub extern "Rust" fn __popcorn_kernel_heap_allocate(layout: Layout) -> Result<NonNull<u8>, AllocError> {
	HEAP.alloc(layout)
}

#[doc(hidden)]
#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_deallocate(ptr: NonNull<u8>, _layout: Layout)  {
	unsafe { HEAP.dealloc(ptr) }
}

/*#[doc(hidden)]
#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_reallocate(ptr: NonNull<u8>, layout: Layout, new_size: usize) -> Result<NonNull<u8>, AllocError>  {
	todo!()
}*/

static HEAP: Heap = Heap {
	#[cfg(not(test))] arenas: RwSpinlock::new(MappedVec::new()),
	#[cfg(test)] arenas: RwSpinlock::new(Vec::new()),
};

struct Heap {
	#[cfg(not(test))] arenas: RwSpinlock<MappedVec<Spinlock<Arena>>>,
	#[cfg(test)] arenas: RwSpinlock<Vec<Spinlock<Arena>>>,
}

impl Heap {
	fn alloc(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
		for arena in &*self.arenas.read() {
			let mut arena = arena.lock();
			match arena.try_alloc(layout) {
				Ok(ptr) => return Ok(ptr),
				Err(_) => continue,
			}
		}

		trace!("allocating new arena");
		let mut new_arena = Arena::with_capacity(layout.size()).map_err(|_| AllocError)?;
		debug!("new arena at {:#x?}", new_arena.bounds());
		let ptr = new_arena.try_alloc(layout).map_err(|_| AllocError)?;
		self.arenas.write().push(Spinlock::new(new_arena));
		
		Ok(ptr)
	}

	unsafe fn dealloc(&self, ptr: NonNull<u8>) {
		let guard = self.arenas.read();

		trace!("dealloc at {ptr:#p}");

		let mut arena = None;
		for arena_iter in &*guard {
			let arena_iter = arena_iter.lock();
			trace!("check arena at {:#x?}", arena_iter.bounds());
			if arena_iter.bounds().contains(&ptr) {
				arena = Some(arena_iter);
			}
		}
		
		// SAFETY: the pointer returned to dealloc must come from this allocator and therefor be in an arena
		let mut arena = unsafe { arena.unwrap_unchecked() }; // we need this to keep the arena locked while modifying it

		unsafe { arena.dealloc(ptr) };
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use alloc::sync::Arc;
	use alloc::vec;
	use alloc::vec::Vec;
	use rand::prelude::{SliceRandom, IndexedRandom};

	#[repr(align(128))]
	struct Foo {
		_a: bool,
		_b: u64,
		_c: isize,
	}

	#[test]
	fn alloc_rand() {
		simple_logger::SimpleLogger::new().init().unwrap();

		let layouts = vec![
			Layout::new::<usize>(),
			Layout::new::<u128>(),
			Layout::new::<Foo>(),
			Layout::new::<Arc<Foo>>(),
		];

		let mut rng = rand::rng();

		let mut allocs = Vec::with_capacity(16);

		for _ in 0..allocs.capacity() {
			let layout = *layouts.choose(&mut rng).unwrap();
			let ptr = HEAP.alloc(layout)
			              .unwrap();
			assert!(ptr.is_aligned_to(layout.align()));
			allocs.push(ptr);
		}

		allocs.shuffle(&mut rng);
		for &alloc in &allocs[0..(allocs.capacity() / 2)] {
			unsafe { HEAP.dealloc(alloc) };
		}

		for _ in 0..allocs.capacity() {
			let layout = *layouts.choose(&mut rng).unwrap();
			let ptr = HEAP.alloc(layout)
			              .unwrap();
			assert!(ptr.is_aligned_to(layout.align()));
			allocs.push(ptr);
		}

		allocs.shuffle(&mut rng);
		for alloc in allocs {
			unsafe { HEAP.dealloc(alloc) };
		}
	}
}
