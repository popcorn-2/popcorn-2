use core::sync::atomic::{AtomicUsize, Ordering};
use log::debug;
use kernel_api::memory::allocator::BackingAllocator;
use kernel_api::memory::{Page, VirtualAddress};
use crate::memory::paging::current_page_table;
use crate::memory::physical::highmem;

// TODO: replace with AtomicPtr or AtomicVirtualAddress
// FIXME: this needs to move out of userspace
static HEAP_END: AtomicUsize = AtomicUsize::new(0x10000);

#[no_mangle]
pub fn __popcorn_adjust_heap(offset: isize) -> (VirtualAddress, isize) {
    let page_count = offset.div_ceil(4096);
    let mut offset = page_count * 4096;

    let old = match offset {
        0 => HEAP_END.load(Ordering::Relaxed),
        ..0 => HEAP_END.fetch_sub(offset.unsigned_abs(), Ordering::Relaxed),
        1.. => HEAP_END.fetch_add(offset.unsigned_abs(), Ordering::Relaxed)
    };

    debug!("Adjusting heap by {offset} bytes = {page_count} pages; old = {old:#x}");

    let old = VirtualAddress::new(old);

    if page_count > 0 {
        let highmem_lock = highmem();
        let base = highmem_lock.allocate_contiguous(page_count as usize).expect("Unable to allocate heap memory");
        for i in 0..(page_count as usize) {
            current_page_table().map_page(
                Page::new(old) + i,
                base + i,
                &*highmem_lock
            ).expect("Unable to map heap pages")
        }
    } else { offset = 0; }
    // todo: shrink heap when

    (old.align_down(), offset)
}
