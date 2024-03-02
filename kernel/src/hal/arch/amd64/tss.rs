use core::arch::asm;
use core::mem;
use kernel_api::memory::VirtualAddress;
use kernel_api::sync::OnceLock;

#[used]
pub static TSS: OnceLock<Tss> = OnceLock::new();

#[repr(C, packed)]
pub struct Tss {
    _res0: u32,
    privilege_stack_table: [VirtualAddress; 3],
    _res1: u64,
    interrupt_stack_table: [VirtualAddress; 7],
    _res2: u64,
    _res3: u16,
    io_map_base: u16,
}

impl Tss {
    pub const fn new() -> Tss {
        Tss {
            _res0: 0,
            privilege_stack_table: [VirtualAddress::new(0); 3],
            _res1: 0,
            interrupt_stack_table: [VirtualAddress::new(0); 7],
            _res2: 0,
            _res3: 0,
            io_map_base: mem::size_of::<Tss>() as u16,
        }
    }

    pub unsafe fn load(gdt_index: u16) {
        asm!("ltr {0:x}", in(reg) gdt_index);
    }
}