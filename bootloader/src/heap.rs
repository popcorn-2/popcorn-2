use core::alloc::{Allocator, AllocError, GlobalAlloc, Layout};
use core::ptr;
use core::ptr::{NonNull, slice_from_raw_parts_mut};
use uefi::prelude::BootServices;
use uefi::table::boot::MemoryType;

static mut BOOT_SERVICES: Option<NonNull<BootServices>> = None;

fn boot_services() -> NonNull<BootServices> {
	unsafe { BOOT_SERVICES.expect("Cannot use allocator after exiting boot services") }
}

struct UefiAllocator<const MEM_TYPE: MemoryType>;

unsafe impl<const MEM_TYPE: MemoryType> Allocator for UefiAllocator<MEM_TYPE> {
	fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
		if layout.size() == 0 { return Err(AllocError); }

		let ret = unsafe { self.alloc(layout) };

		return if ret.is_null() { Err(AllocError) }
		else { Ok(unsafe {
			NonNull::new_unchecked(slice_from_raw_parts_mut(ret, layout.size()))
		}) }
	}

	unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
		self.dealloc(ptr.as_ptr(), layout);
	}
}

unsafe impl<const MEM_TYPE: MemoryType> GlobalAlloc for UefiAllocator<MEM_TYPE> {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		let size = layout.size();
		let align = layout.align();

		if align > 8 {
			// The requested alignment is greater than 8, but `allocate_pool` is
			// only guaranteed to provide eight-byte alignment. Allocate extra
			// space so that we can return an appropriately-aligned pointer
			// within the allocation.
			let full_alloc_ptr = if let Ok(ptr) = boot_services()
					.as_ref()
					.allocate_pool(MEM_TYPE, size + align)
			{
				ptr
			} else {
				return ptr::null_mut();
			};

			// Calculate the offset needed to get an aligned pointer within the
			// full allocation. If that offset is zero, increase it to `align`
			// so that we still have space to store the extra pointer described
			// below.
			let mut offset = full_alloc_ptr.align_offset(align);
			if offset == 0 {
				offset = align;
			}

			// Before returning the aligned allocation, store a pointer to the
			// full unaligned allocation in the bytes just before the aligned
			// allocation. We know we have at least eight bytes there due to
			// adding `align` to the memory allocation size. We also know the
			// write is appropriately aligned for a `*mut u8` pointer because
			// `align_ptr` is aligned, and alignments are always powers of two
			// (as enforced by the `Layout` type).
			let aligned_ptr = full_alloc_ptr.add(offset);
			(aligned_ptr.cast::<*mut u8>()).sub(1).write(full_alloc_ptr);
			aligned_ptr
		} else {
			// The requested alignment is less than or equal to eight, and
			// `allocate_pool` always provides eight-byte alignment, so we can
			// use `allocate_pool` directly.
			boot_services()
					.as_ref()
					.allocate_pool(MEM_TYPE, size)
					.unwrap_or(ptr::null_mut())
		}
	}

	unsafe fn dealloc(&self, mut ptr: *mut u8, layout: Layout) {
		if layout.align() > 8 {
			// Retrieve the pointer to the full allocation that was packed right
			// before the aligned allocation in `alloc`.
			ptr = (ptr as *const *mut u8).sub(1).read();
		}
		boot_services().as_ref().free_pool(ptr).unwrap();
	}
}

#[global_allocator]
static ALLOCATOR: UefiAllocator<{super::memory_types::LOADER_HEAP}> = UefiAllocator;
