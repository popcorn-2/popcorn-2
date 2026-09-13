use core::arch::asm;
use core::cell::SyncUnsafeCell;
use core::ptr::addr_of;
use kernel_api::memory::VirtualAddress;
use kernel_api::sync::OnceLock;

// todo: make this thread_local
pub static TSS: OnceLock<Tss> = OnceLock::new();

#[repr(transparent)]
struct StaticStack([usize; 3*4096/size_of::<usize>()]);

static DOUBLE_FAULT_STACK: SyncUnsafeCell<StaticStack> = SyncUnsafeCell::new(StaticStack([0; 4096*3/size_of::<usize>()]));

#[repr(C, packed(4))]
pub struct Tss {
    _res0: u32,
    // use SyncUnsafeCell so we can modify through a shared reference
    privilege_stack_table: [SyncUnsafeCell<VirtualAddress>; 3],
    _res1: u64,
    interrupt_stack_table: [VirtualAddress; 7],
    _res2: u64,
    _res3: u16,
    io_map_base: u16,
}

impl Tss {
    pub fn new() -> Tss {
        Tss {
            _res0: 0,
            privilege_stack_table: [const { SyncUnsafeCell::new(VirtualAddress::new(0)) }; 3],
            _res1: 0,
            interrupt_stack_table: [
                VirtualAddress::from(unsafe { DOUBLE_FAULT_STACK.get().add(1) }),
                VirtualAddress::new(0),
                VirtualAddress::new(0),
                VirtualAddress::new(0),
                VirtualAddress::new(0),
                VirtualAddress::new(0),
                VirtualAddress::new(0),
            ],
            _res2: 0,
            _res3: 0,
            io_map_base: size_of::<Tss>() as u16,
        }
    }
    
    #[inline]
    pub extern "C" fn set_rsp0(&self, addr: VirtualAddress) {
        assert!(addr.is_aligned_to(16), "rsp0 must be 16 byte aligned");

        unsafe {
            asm!("lock xchg qword ptr [{}], {}", in(reg) addr_of!(self.privilege_stack_table[0]), inout(reg) addr.addr => _);
        }
    }

    pub unsafe fn load(gdt_index: u16) {
        unsafe { asm!("ltr {0:x}", in(reg) gdt_index) };
    }
}
