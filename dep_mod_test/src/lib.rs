#![no_std]

extern crate kernel_exports;

#[no_mangle]
#[inline(never)]
pub fn add(left: usize, right: usize) -> usize {
    left + right
}
