use core::sync::atomic::{AtomicUsize, Ordering};
use log::debug;
use kernel_api::memory::VirtualAddress;

// TODO: replace with AtomicPtr or AtomicVirtualAddress
static HEAP_END: AtomicUsize = AtomicUsize::new(0x10000);

#[no_mangle]
pub fn __popcorn_adjust_heap(offset: isize) -> (VirtualAddress, isize) {
    let page_count = offset.div_ceil(4096);

    debug!("Adjusting heap by {offset} bytes = {page_count} pages");

    let old = match offset {
        0 => HEAP_END.load(Ordering::Relaxed),
        ..0 => HEAP_END.fetch_sub(offset.unsigned_abs(), Ordering::Relaxed),
        1.. => HEAP_END.fetch_add(offset.unsigned_abs(), Ordering::Relaxed)
    };
    (VirtualAddress::new(old), page_count)
}
