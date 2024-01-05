#![no_std]

#![feature(kernel_heap)]
#![feature(kernel_address_alignment_runtime)]
#![feature(kernel_sync_once)]
#![feature(kernel_mmap)]
#![feature(int_roundings)]

use core::alloc::Layout;
use core::fmt::{Debug};
use core::mem::MaybeUninit;
use core::ptr::{NonNull};
use kernel_api::memory::heap::{adjust_heap, Heap};
use kernel_api::memory::{Page, VirtualAddress, AllocError};
use kernel_api::sync::{LazyLock, Mutex};
use log::debug;
use kernel_api::memory::mapping::Mapping;

//const _: () = {
    static KERNEL_HEAP: LazyLock<SyncHeap> = LazyLock::new(SyncHeap::new);

    #[no_mangle]
    pub extern "Rust" fn __popcorn_kernel_heap_allocate(layout: Layout) -> Result<NonNull<u8>, AllocError> {
        <SyncHeap as Heap>::allocate(&KERNEL_HEAP, layout)
    }

    #[no_mangle]
    pub unsafe extern "Rust" fn __popcorn_kernel_heap_deallocate(ptr: NonNull<u8>, layout: Layout)  {
        <SyncHeap as Heap>::deallocate(&KERNEL_HEAP, ptr, layout)
    }
//};

#[derive(Debug)]
struct SyncHeap(Mutex<BadHeap>);

#[derive(Debug)]
struct BadHeap {
    watermark: VirtualAddress,
    mapping: Mapping
}

impl Heap for SyncHeap {
    fn new() -> Self where Self: Sized {
        let mapping = Mapping::new(0).unwrap();

        Self(Mutex::new(BadHeap {
            watermark: mapping.end().start().align_down(),
            mapping
        }))
    }

    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        debug!("allocate {layout:?}");

        let mut guard = self.0.lock();
        let start = guard.watermark.align_up_runtime(layout.align());
        let end = start + layout.size();

        let max_addr = guard.mapping.end().start();
        if end > max_addr {
            debug!("Increment heap end");
            let increment = isize::try_from(end - max_addr).map_err(|_| AllocError)?;
            let increment = increment.div_ceil(4096);
            let new_len = guard.mapping.len() + increment.unsigned_abs();
            debug!("Trying to remap");
            guard.mapping.remap_in_place(new_len)?;
        }

        guard.watermark = end;
        Ok(NonNull::new(start.as_ptr()).unwrap())
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, _: Layout) {
        if let Some(guard) = self.0.try_lock() {
            debug_assert!(guard.watermark.as_ptr() >= ptr.as_ptr(), "Out of range pointer was freed");
        }
    }
}
