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
use kernel_api::kernel_module;
use kernel_api::sync::{RwSpinlock, Spinlock};
#[cfg(not(test))] use crate::mapped_vec::MappedVec;
#[cfg(test)] extern crate alloc;
#[cfg(test)] use alloc::vec::Vec;

mod mapped_vec;
mod arena;

kernel_module! {
	type: Heap,
	name: "Popcorn2 Arena Heap",
	author: "Eliyahu Gluschove-Koppel <egkoppel@eliyahu.co.uk>",
	license: "MPL-2.0",
}

impl kernel_api::modules::Module for Heap {
	fn early_init() -> Result<(), &'static str> { Ok(()) }
	fn late_init() -> Result<(), &'static str> { Ok(()) }
}

#[doc(hidden)]
#[unsafe(no_mangle)]
pub extern "Rust" fn __popcorn_kernel_heap_allocate(layout: Layout) -> Result<NonNull<u8>, AllocError> {
	HEAP.alloc(layout)
}

/// # Safety
///
/// `ptr` must have previously been allocated by a call to `__popcorn_kernel_heap_allocate`.
#[doc(hidden)]
#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_deallocate(ptr: NonNull<u8>, _layout: Layout)  {
	// SAFETY: Caller upholds that `ptr` came from a call to `__popcorn_kernel_heap_allocate`, which only returns pointers from `HEAP.alloc()`
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
	/// # Errors
	///
	/// Returns [`AllocError`] if no memory was available for the requested allocation.
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

	/// # Safety
	///
	/// `ptr` must have previously been allocated by a call to `alloc`.
	unsafe fn dealloc(&self, ptr: NonNull<u8>) {
		let guard = self.arenas.read();

		trace!("dealloc at {ptr:#p}");

		let mut arena = 'found: {
			for arena_iter in &*guard {
				let arena_iter = arena_iter.lock();
				trace!("check arena at {:#x?}", arena_iter.bounds());
				if arena_iter.bounds().contains(&ptr) {
					break 'found arena_iter;
				}
			}
			unreachable!("`ptr` not found in any arenas in this allocator");
		};

		// SAFETY: Caller upholds that `ptr` came from a call to `alloc`, which only returns pointers from `arena.try_alloc()`
		unsafe { arena.dealloc(ptr) };
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use alloc::sync::Arc;
	use alloc::vec;
	use log::LevelFilter;
	use rand::prelude::{SliceRandom, IndexedRandom};

	#[repr(align(128))]
	struct Foo {
		_a: bool,
		_b: u64,
		_c: isize,
	}

	#[test]
	fn alloc_rand() {
		simple_logger::SimpleLogger::new()
			.with_level(LevelFilter::Debug)
			.with_module_level("kernel_api", LevelFilter::Error)
			.init()
			.unwrap();

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
			debug!("alloc at {ptr:p}");
			unsafe { ptr.write_bytes(0x36, layout.size()) };
			allocs.push(ptr);
		}

		allocs.shuffle(&mut rng);
		for alloc in allocs.drain(..(allocs.capacity() / 2)) {
			debug!("dealloc at {alloc:p}");
			unsafe { HEAP.dealloc(alloc) };
		}

		for _ in 0..allocs.capacity() {
			let layout = *layouts.choose(&mut rng).unwrap();
			let ptr = HEAP.alloc(layout)
			              .unwrap();
			assert!(ptr.is_aligned_to(layout.align()));
			debug!("alloc at {ptr:p}");
			unsafe { ptr.write_bytes(0x36, layout.size()) };
			allocs.push(ptr);
		}

		allocs.shuffle(&mut rng);
		for alloc in allocs {
			debug!("dealloc at {alloc:p}");
			unsafe { HEAP.dealloc(alloc) };
		}
	}
}
