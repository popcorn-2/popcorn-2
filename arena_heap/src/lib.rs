#![no_std]
#![forbid(unsafe_op_in_unsafe_fn)]

#![feature(allocator_api)]
#![feature(kernel_mmap_to_parts)]
#![feature(let_chains)]

use core::alloc::{AllocError, Layout};
use core::ptr::NonNull;
use log::debug;
use arena::Arena;
use chunk::ChunkHeader;
use kernel_api::dbg;
use kernel_api::sync::{RwSpinlock, Spinlock};
use crate::mapped_vec::MappedVec;

mod chunk;
mod arena;
mod mapped_vec;

#[unsafe(no_mangle)]
pub extern "Rust" fn __popcorn_kernel_heap_allocate(layout: Layout) -> Result<NonNull<u8>, AllocError> {
	HEAP.alloc(layout)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn __popcorn_kernel_heap_deallocate(ptr: NonNull<u8>, layout: Layout)  {
	
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

	unsafe fn realloc(&self, ptr: NonNull<u8>) {
		// fixme(provenance): this is invalid
		let chunk = unsafe { ptr.byte_sub(size_of::<ChunkHeader>()).cast::<ChunkHeader>() };
	}

	unsafe fn dealloc(&self, ptr: NonNull<u8>) {

	}
}
