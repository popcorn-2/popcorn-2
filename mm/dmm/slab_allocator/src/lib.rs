#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]
#![feature(slice_ptr_get)]
#![feature(pointer_byte_offsets)]
#![feature(strict_provenance)]
#![feature(new_uninit)]
#![feature(pointer_is_aligned)]

extern crate alloc;

pub mod linked_list;

use core::marker::PhantomData;
use core::mem;
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use more_asserts::assert_ge;
use crate::linked_list::LinkedList;

const SLAB_SIZE: usize = 4096;

pub trait Cacheable: Default {
    fn recycle(&mut self) {}
}

pub struct SlabAllocator<'vmm, T: Cacheable> {
    virtual_allocator: &'vmm dyn VirtualMemoryAllocator,
    slabs: linked_list::LinkedList<SlabMetadata>
    _phantom: PhantomData<T>
}

pub trait VirtualMemoryAllocator {}

impl<'vmm, T: Cacheable> SlabAllocator<'vmm, T> {
    const ALLOC_SIZE: usize = mem::size_of::<T>().next_power_of_two();

    pub fn new(vmm: &dyn VirtualMemoryAllocator) -> Self {
        todo!()
    }

    pub fn alloc(&self) -> Result<NonNull<T>, AllocError> {

    }

    /// # Safety
    /// Pointer must have been allocated by this allocator
    /// Size must be the original allocation size
    pub unsafe fn dealloc(&self, ptr: NonNull<T>) {
        todo!()
    }
}

type EmptyBlock = linked_list::Node<()>;

#[derive(Debug)]
struct SlabMetadata {
    block_free_list: linked_list::LinkedList<()>,
    slab: NonNull<[u8]>
}

impl SlabMetadata {


    unsafe fn new(allocation: NonNull<[u8]>, block_size_log2: usize) -> &'slab mut linked_list::Node<SlabMetadata> {
        use linked_list::Node;

        let block_size = 1 << block_size_log2;
        assert_ge!(block_size, mem::size_of::<EmptyBlock>());
        assert!(allocation.as_mut_ptr().is_aligned_to(block_size));

        let block_count = allocation.len() / block_size;
        if block_count < 8 {
            // Large blocks - store metadata outside
            let mut previous_block = None;
            for block_backing in allocation.chunks_exact_mut(block_size) {
                // SAFETY: Safe to assume random memory is uninitialised
                // We assert that the block array starts on an aligned pointer therefore since `size == align`,
                // each subsequent block is also aligned
                // Pointer was generated from a `&mut [u8]` so must point to valid memory
                let block = unsafe { &mut *(block_backing.as_mut_ptr() as *mut MaybeUninit<EmptyBlock>) };
                let block = block.write(Node::new((), previous_block));
                previous_block = Some(NonNull::from(block));
            }

            let metadata: &mut Node<SlabMetadata> = todo!();
            metadata.data = SlabMetadata {
                block_free_list: LinkedList::from(previous_block),
                slab: NonNull::from(allocation),
                _phantom: PhantomData
            };

            metadata
        } else {
            // Small blocks - store metadata inside
            const METADATA_SIZE: usize = mem::size_of::<Node<SlabMetadata>>();
            const METADATA_ALIGN: usize = mem::align_of::<Node<SlabMetadata>>();

            // SAFETY: Resulting pointer is one past the end of the slice which is legal
            let allocation_end_ptr = unsafe { allocation.as_mut_ptr().byte_add(allocation.len()) };
            let metadata_start_pointer = unsafe {
                // SAFETY: Pointer gets manually aligned and we assert that its within the original allocation
                let unaligned = allocation_end_ptr.byte_sub(METADATA_SIZE);
                let aligned = (unaligned.addr()) & !(METADATA_ALIGN);
                assert_ge!(aligned, allocation.as_mut_ptr().addr());
                aligned
            };
            let difference = allocation_end_ptr.addr() - metadata_start_pointer;
            let (slab_data, metadata_storage) = allocation.split_at_mut(allocation.len() - difference);

            let mut previous_block = None;
            for block_backing in slab_data.chunks_exact_mut(block_size) {
                // SAFETY: Safe to assume random memory is uninitialised
                // We assert that the block array starts on an aligned pointer therefore since `size == align`,
                // each subsequent block is also aligned
                // Pointer was generated from a `&mut [u8]` so must point to valid memory
                let block = unsafe { &mut *(block_backing.as_mut_ptr() as *mut MaybeUninit<EmptyBlock>) };
                let block = block.write(Node::new((), previous_block));
                previous_block = Some(NonNull::from(block));
            }

            let metadata = SlabMetadata {
                block_free_list: LinkedList::from(previous_block),
                slab: NonNull::from(slab_data),
                _phantom: PhantomData
            };

            // SAFETY: We aligned this pointer at generation from a valid allocation
            // and safe to assume it is uninitialized
            let metadata_storage = unsafe {
                &mut *(metadata_storage.as_mut_ptr() as *mut MaybeUninit<Node<SlabMetadata>>)
            };
            metadata_storage.write(Node::new(metadata, None))
        }
    }
}

/*
0x000         0x400         0x800         0xc00         0x1000
   ______________________________________________________________________
  | EMPTY BLOCK | EMPTY BLOCK | EMPTY BLOCK | EMPTY BLOCK | METADATA     |
  | next: 0x400 | next: 0x800 | next: 0xc00 | next: None  | first: 0x000 |
  |_____________|_____________|_____________|_____________|______________|
 */

pub struct AllocError;

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use core::ops::DerefMut;
    use crate::SlabMetadata;

    #[repr(align(4096), C)]
    struct AlignedPage([u8; 4096]);

    #[test]
    fn foo() {
        let mut backing = unsafe {
            let b = Box::<AlignedPage>::new_zeroed();
            b.assume_init()
        };
        let baz = SlabMetadata::new(&mut backing.deref_mut().0, 8);
        println!("{baz:?}");
    }

    #[test]
    fn just_enough_space() {
        let mut backing = unsafe {
            let b = Box::<AlignedPage>::new_zeroed();
            b.assume_init()
        };
        let baz = SlabMetadata::new(&mut backing.deref_mut().0[0..64], 3);
        println!("{baz:?}");
    }
}
