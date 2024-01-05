//! Provides an interface to act as the kernel heap

#![unstable(feature = "kernel_heap", issue = "4")]

use core::alloc::Layout;
use core::cmp::min;
use core::ptr;
use core::ptr::NonNull;
use super::AllocError;

/// A heap manager
pub trait Heap {
    /// Creates a new instance of the heap manager
    fn new() -> Self where Self: Sized;

    /// Allocates a new area of memory for `layout`
    ///
    /// # Errors
    ///
    /// Returns [`AllocError`] if the memory could not be allocated
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError>;

    /// Deallocates the memory pointed to by `ptr`
    ///
    /// # Safety
    ///
    /// The `ptr` must have been previously allocated by the same allocator with the same layout as `layout`
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);

    /// Adjusts the size of the allocation pointed to by `ptr`
    ///
    /// # Safety
    ///
    /// The `ptr` must have been previously allocated by the same allocator with the same layout as `old_layout`
    ///
    /// # Errors
    ///
    /// If the size could not be adjusted, returns [`AllocError`] and the old pointer is still valid to the original allocation.
    unsafe fn reallocate(&self, ptr: NonNull<u8>, old_layout: Layout, new_size: usize) -> Result<NonNull<u8>, AllocError> {
        let new_layout = Layout::from_size_align(new_size, old_layout.align())
            .expect("Original layout was somehow invalid?");

        let new = self.allocate(new_layout)?;
        unsafe {
            ptr::copy_nonoverlapping(ptr.as_ptr(), new.as_ptr(), min(old_layout.size(), new_size));
        }
        Ok(new)
    }
}
