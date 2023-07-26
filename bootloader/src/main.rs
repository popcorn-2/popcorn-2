#![feature(ptr_metadata)]
#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(pointer_is_aligned)]
#![feature(new_uninit)]
#![feature(split_array)]
#![feature(slice_ptr_len)]
#![feature(slice_ptr_get)]
#![feature(pointer_byte_offsets)]
#![feature(inline_const)]
#![feature(type_name_of_val)]
#![feature(arbitrary_self_types)]
#![feature(concat_bytes)]
#![feature(adt_const_params)]
#![feature(allocator_api)]
#![feature(iter_collect_into)]
#![no_main]
#![no_std]

extern crate alloc;

mod framebuffer;
mod config;
//mod ui;
//mod loadingscreen;
//mod elf;
mod paging;
mod logging;
use uefi::prelude::*;

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    loop {}
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    error!("{}", info);
    loop {}
}

/*
| UEFI type                  | Use                                                              |
|----------------------------|------------------------------------------------------------------|
| EfiReservedMemoryType      | Unusable memory                                                  |
| EfiLoaderCode              | Bootloader code                                                  |
| EfiLoaderData              | Bootloader data and memory allocations by bootloader             |
| EfiBootServicesCode        | Boot services driver code - preserve to use boot services        |
| EfiBootServicesData        | Boot services driver data - preserve to use boot services        |
| EfiRuntimeServicesCode     | Runtime services driver code - preserve to use runtime services |
| EfiRuntimeServicesData     | Runtime services driver data - preserve to use runtime services |
| EfiConventionalMemory      | Free memory                                                      |
| EfiUnusableMemory          | Memory with errors detected                                      |
| EfiACPIReclaimMemory       | Memory containing ACPI tables - preserve until parsing ACPI      |
| EfiACPIMemoryNVS           | ACPI firmware memory that must be preserved across sleep         |
| EfiMemoryMappedIO          | ???                                                              |
| EfiMemoryMappedIOPortSpace | ???                                                              |
| EfiPalCode                 | ???                                                              |
| EfiPersistentMemory        | Non-volatile but otherwise conventional memory                   |
| EfiUnacceptedMemoryType    | ???                                                              |
 */
