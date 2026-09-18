use core::cell::Cell;
use core::sync::atomic::AtomicPtr;
use kernel_api::memory::{VirtualAddress, PAGE_SIZE};

struct StaticStack([u8; PAGE_SIZE * 3]);

static mut BSP_DOUBLE_FAULT_STACK: StaticStack = StaticStack([0; PAGE_SIZE * 3]);

#[repr(C, align(8))]
pub struct Tss(pub(super) TssInner);

#[repr(C, packed)]
pub(super) struct TssInner {
	_reserved: u32,
	pub(super) rsp0: Cell<*mut u8>,
	// rings 1 and 2 are unused so RSP1/2 space is used as scratch memory
	_pad1: u32,
	pub(super) scratch: Cell<u64>,
	_pad2: u32,
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
	pub const INIT: Self = {
		let mut tss = Tss::new();
		let stack = &raw mut BSP_DOUBLE_FAULT_STACK;
		let stack_end = unsafe { stack.add(1) };
		tss.0.ist1 = AtomicPtr::new(stack_end.cast());
		tss
	};

	const fn new() -> Self {
		Tss(TssInner {
			_reserved: 0,
			rsp0: Cell::new(core::ptr::null_mut()),
			_pad1: 0,
			scratch: Cell::new(0),
			_pad2: 0,
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
		})
	}

	pub fn set_rsp0(&self, value: VirtualAddress) {
		unsafe {
			core::arch::asm!(
				"lock xchg qword ptr [{}], {}",
				in(reg) &raw const self.0.rsp0,
				inout(reg) value.addr => _,
			);
		}
	}

	pub fn set_scratch(&self, value: u64) {
		debug_assert!((&raw const self.0.scratch).is_aligned());
		unsafe {
			core::arch::asm!(
				"mov qword ptr [{}], {}",
				in(reg) &raw const self.0.scratch,
				in(reg) value,
			);
		}
	}

	pub fn get_scratch(&self) -> u64 {
		debug_assert!((&raw const self.0.scratch).is_aligned());
		let value;
		unsafe {
			core::arch::asm!(
				"mov {}, qword ptr [{}]",
				out(reg) value,
				in(reg) &raw const self.0.scratch,
			);
		}
		value
	}
}
