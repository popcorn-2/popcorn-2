#![no_std]

#![feature(int_roundings)]

#![deny(warnings)]

use core::alloc::Layout;
use core::fmt::Debug;
use core::num::NonZero;
use core::ptr::NonNull;
use kernel_api::memory::heap::Heap;
use kernel_api::memory::{VirtualAddress, AllocError};
use kernel_api::sync::{LazyLock, Spinlock};
use log::debug;
use kernel_api::memory::mapping::{Config, Mapping};
use kernel_api::memory::r#virtual::Global;

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
struct SyncHeap(Spinlock<BadHeap>);

#[derive(Debug)]
struct BadHeap {
    watermark: VirtualAddress,
    mapping: Option<Mapping<'static>>,
}

impl Heap for SyncHeap {
    fn new() -> Self where Self: Sized {
        Self(Spinlock::new(BadHeap {
            watermark: VirtualAddress::new(0),
            mapping: None,
        }))
    }

    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        debug!("allocate {layout:?}");

        let guard = &mut *self.0.lock();
        
        let Some(size) = NonZero::new(layout.size()) else { return Ok(NonNull::dangling()); };
        
        let start = if let Some(mapping) = &mut guard.mapping {
            let start = guard.watermark.align_up_runtime(layout.align());
            let end = start + size.get();
            let heap_end = mapping.virtual_valid_end().start();
            if end > heap_end {
                debug!("Increment heap end");
                let increment = isize::try_from(end - heap_end).map_err(|_| AllocError)?
                        .div_ceil(4096);
                let new_len = mapping.physical_len().checked_add(increment.unsigned_abs()).ok_or(AllocError)?;
                debug!("Trying to remap");
                mapping.resize_in_place(new_len)?;
            }
            start
        } else {
            let page_count = NonZero::new(size.get().div_ceil(4096)).unwrap();
            let mapping = Mapping::new(Config::new(page_count), 25)?;
            let start = mapping.virtual_valid_start().start().align_up::<1>();
            guard.mapping = Some(mapping);
            start
        };

        guard.watermark = start + size.get();
        Ok(NonNull::new(start.as_ptr()).unwrap())
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, _: Layout) {
        if let Some(guard) = self.0.try_lock() {
            debug_assert!(guard.watermark.as_ptr() >= ptr.as_ptr(), "Out of range pointer was freed");
        }
    }
}
