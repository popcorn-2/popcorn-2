use core::sync::atomic::AtomicPtr;
use kernel_api::memory::{VirtualAddress, PAGE_SIZE};

pub static BSP_TSS: Tss = {
	let mut tss = Tss::new();
	let stack = &raw mut BSP_DOUBLE_FAULT_STACK;
	let stack_end = unsafe { stack.add(1) };
	tss.ist1 = AtomicPtr::new(stack_end.cast());
	tss
};

struct StaticStack([u8; PAGE_SIZE * 3]);

static mut BSP_DOUBLE_FAULT_STACK: StaticStack = StaticStack([0; PAGE_SIZE * 3]);

#[repr(C, packed(4))]
pub struct Tss {
	_reserved: u32,
	rsp0: AtomicPtr<u8>,
	rsp1: AtomicPtr<u8>,
	rsp2: AtomicPtr<u8>,
	_reserved1: u64,
	ist1: AtomicPtr<u8>,
	ist2: AtomicPtr<u8>,
	ist3: AtomicPtr<u8>,
	ist4: AtomicPtr<u8>,
	ist5: AtomicPtr<u8>,
	ist6: AtomicPtr<u8>,
	ist7: AtomicPtr<u8>,
	_reserved2: u64,
	_reserved3: u16,
	iopb: u16,
}

impl Tss {
	const fn new() -> Self {
		Self {
			_reserved: 0,
			rsp0: AtomicPtr::new(core::ptr::null_mut()),
			rsp1: AtomicPtr::new(core::ptr::null_mut()),
			rsp2: AtomicPtr::new(core::ptr::null_mut()),
			_reserved1: 0,
			ist1: AtomicPtr::new(core::ptr::null_mut()),
			ist2: AtomicPtr::new(core::ptr::null_mut()),
			ist3: AtomicPtr::new(core::ptr::null_mut()),
			ist4: AtomicPtr::new(core::ptr::null_mut()),
			ist5: AtomicPtr::new(core::ptr::null_mut()),
			ist6: AtomicPtr::new(core::ptr::null_mut()),
			ist7: AtomicPtr::new(core::ptr::null_mut()),
			_reserved2: 0,
			_reserved3: 0,
			iopb: size_of::<Self>().truncate(),
		}
	}

	pub fn set_rsp0(&self, value: VirtualAddress) {
		unsafe {
			core::arch::asm!(
				"lock xchg qword ptr [{}], {}",
				in(reg) &raw const self.rsp0,
				inout(reg) value.addr => _,
			);
		}
	}
}
