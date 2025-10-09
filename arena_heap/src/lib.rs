#![no_std]
#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(warnings)]

#![feature(allocator_api)]
#![feature(debug_closure_helpers)]
#![feature(pointer_is_aligned_to)]
#![feature(new_zeroed_alloc)]
#![cfg_attr(feature = "kasan", feature(sanitize))]

use core::alloc::{AllocError, Layout};
use core::ptr::NonNull;
use log::debug;
use arena::Arena;
use chunk::ChunkHeader;
use kernel_api::dbg;
use kernel_api::sync::{RwSpinlock, Spinlock};
use crate::mapped_vec::MappedVec;
#[cfg(feature = "kasan")] use kernel_api::memory::asan::{set_shadow_heap_free, mem_to_shadow};
#[cfg(test)] extern crate alloc;

mod chunk;
mod arena;
mod mapped_vec;

#[unsafe(no_mangle)]
pub extern "Rust" fn __popcorn_kernel_heap_allocate(layout: Layout) -> Result<NonNull<u8>, AllocError> {
	HEAP.alloc(layout)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_deallocate(ptr: NonNull<u8>, _layout: Layout)  {
	unsafe { HEAP.dealloc(ptr) }
}

/*#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_reallocate(ptr: NonNull<u8>, layout: Layout, new_size: usize) -> Result<NonNull<u8>, AllocError>  {
	todo!()
}*/

static HEAP: Heap = Heap {
	arenas: RwSpinlock::new(MappedVec::new()),
};

struct Heap {
	arenas: RwSpinlock<MappedVec<Spinlock<Arena>>>,
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

		debug!("allocating new arena");
		let mut new_arena = Arena::with_capacity(layout.size()).map_err(|_| AllocError)?;
		let ptr = dbg!(new_arena.try_alloc(layout).map_err(|_| AllocError))?;
		self.arenas.write().push(Spinlock::new(new_arena));
		
		Ok(ptr)
	}

	unsafe fn dealloc(&self, mut ptr: NonNull<u8>) {
		let guard = self.arenas.read();

		let mut arena = None;
		for arena_iter in &*guard {
			let arena_iter = arena_iter.lock();
			if arena_iter.bounds().contains(&ptr) {
				ptr = arena_iter.bounds().start.with_addr(ptr.addr()); // get pointer with wider provenance to cover whole arena instead of
				                                          // just allocation (which excludes the chunk headers too)
				arena = Some(arena_iter);
			}
		}
		
		// SAFETY: the pointer returned to dealloc must come from this allocator and therefor be in an arena
		let _guard = unsafe { arena.unwrap_unchecked() }; // we need this to keep the arena locked while modifying it
		
		let chunk_header = unsafe { ptr.cast::<ChunkHeader>().offset(-1).as_mut() };
		chunk_header.set_busy(false);

		#[cfg(feature = "kasan")] unsafe {
			set_shadow_heap_free(
				mem_to_shadow(ptr.as_ptr().into()),
				chunk_header.size().div_ceil(8),
			);
		}
		
		let next = unsafe { chunk_header.next().expect("can't free sentinel chunk").as_mut() };
		let prev = unsafe { chunk_header.prev().map(|mut ptr| ptr.as_mut()) };
		
		if !next.busy() && next.next().is_some() { // don't merge with sentinel chunk
			debug!("merging right");
			chunk_header.set_next(next.next());
			unsafe {
				next.next().expect("just checked this is some")
						.as_mut()
						.set_prev(next.next());
			}
			#[cfg(feature = "generations")] next.overwrite_magic();
		}
		
		if let Some(prev) = prev && !prev.busy() {
			debug!("merging left");
			prev.set_next(chunk_header.next()); // grab new next in case we merged right already
			unsafe {
				chunk_header.next().expect("can't free sentinel chunk").as_mut()
			}.set_prev(Some(NonNull::from(prev)));
			#[cfg(feature = "generations")] chunk_header.overwrite_magic(); // even if `prev` is completely free, do this unconditionally cause easier and won't make a difference
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn foo() {
		let _ = HEAP.alloc(Layout::new::<u128>());
	}
}
