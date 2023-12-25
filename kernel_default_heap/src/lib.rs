#![no_std]

#![feature(kernel_heap)]
#![feature(kernel_address_alignment_runtime)]
#![feature(kernel_sync_once)]

use core::alloc::Layout;
use core::fmt::{Debug};
use core::mem::MaybeUninit;
use core::ptr::{NonNull};
use kernel_api::memory::heap::{adjust_heap, Heap};
use kernel_api::memory::{Page, VirtualAddress, AllocError};
use kernel_api::sync::{LazyLock, Mutex};
use log::debug;

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
    max_address: VirtualAddress
}

impl Heap for SyncHeap {
    fn new() -> Self where Self: Sized {
        let (watermark, _) = adjust_heap(0);

        Self(Mutex::new(BadHeap {
            watermark,
            max_address: watermark
        }))
    }

    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        debug!("allocate {layout:?}");

        let mut guard = self.0.lock();
        let start = guard.watermark.align_up_runtime(layout.align());
        let end = start + layout.size();

        if end > guard.max_address {
            let increment = isize::try_from(end - guard.max_address).map_err(|_| AllocError)?;
            let (_, increment) = adjust_heap(increment);

            guard.max_address = guard.max_address + increment.unsigned_abs();
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
