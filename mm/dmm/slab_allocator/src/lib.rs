#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]
#![feature(slice_ptr_get)]
#![feature(pointer_byte_offsets)]
#![feature(strict_provenance)]
#![feature(new_uninit)]
#![feature(pointer_is_aligned)]

extern crate alloc;

pub mod linked_list;
