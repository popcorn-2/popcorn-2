use core::alloc::{GlobalAlloc, Layout};

extern "Rust" {
	fn __popcorn_module_panic(info: &core::panic::PanicInfo) -> !;
	fn __popcorn_module_alloc(layout: Layout) -> *mut u8;
	fn __popcorn_module_dealloc(ptr: *mut u8, layout: Layout);
	fn __popcorn_module_alloc_zeroed(layout: Layout) -> *mut u8;
	fn __popcorn_module_realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8;
}

#[cfg(all(feature = "panic", not(feature = "test")))]
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! { unsafe { __popcorn_module_panic(info) } }

struct KernelAllocator;
unsafe impl GlobalAlloc for KernelAllocator {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 { __popcorn_module_alloc(layout) }
	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { __popcorn_module_dealloc(ptr, layout) }
	unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 { __popcorn_module_alloc_zeroed(layout) }
	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 { __popcorn_module_realloc(ptr, layout, new_size) }
}

#[cfg(all(feature = "alloc", not(feature = "test")))]
#[global_allocator]
static ALLOCATOR: KernelAllocator = KernelAllocator;
