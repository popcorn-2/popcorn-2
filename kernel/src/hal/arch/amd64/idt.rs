use core::arch::asm;
use core::ops::{Index, IndexMut};

pub mod handler {
	use bitflags::bitflags;
	bitflags! {
		#[derive(Debug)]
		pub struct PageFaultError: u32 {
			const PAGE_PRESENT = 1<<0;
			const ATTEMPTED_WRITE = 1<<1;
			const USER_FAIL = 1<<2;
			const RESERVED_BIT_IN_PT = 1<<3;
			const INSTRUCTION_FETCH = 1<<4;
			const PROTECTION_KEY = 1<<5;
			const SHADOW_STACK = 1<<6;
			const RMP_VIOLATION = 1<<31;
		}
	}

	#[derive(Debug)]
	#[repr(u32)]
	pub enum ControlFlowError {
		NearReturn = 1,
		FarRet_IRet = 2,
		InvalidShadowStackRestore = 3,
		InvalidShadowStackBusy = 4,
	}

	#[derive(Debug)]
	#[repr(C)]
	pub struct InterruptStackFrame {
		pub instruction_pointer: u64,
		pub code_segment: u64,
		pub cpu_flags: u64,
		pub stack_pointer: u64,
		pub stack_segment: u64,
	}

	pub type Normal = extern "x86-interrupt" fn(InterruptStackFrame);
	pub type NormalWithError = extern "x86-interrupt" fn(InterruptStackFrame, u32);
	pub type Diverging = extern "x86-interrupt" fn(InterruptStackFrame) -> !;
	pub type DivergingWithError = extern "x86-interrupt" fn(InterruptStackFrame, u32) -> !;
	pub type PageFault = extern "x86-interrupt" fn(InterruptStackFrame, PageFaultError);
	pub type ControlFlow = extern "x86-interrupt" fn(InterruptStackFrame, ControlFlowError);

	pub trait Handler {
		fn addr(&self) -> *const ();
	}

	impl Handler for Normal {
		fn addr(&self) -> *const () { *self as *const () }
	}

	impl Handler for NormalWithError {
		fn addr(&self) -> *const () { *self as *const () }
	}

	impl Handler for Diverging {
		fn addr(&self) -> *const () { *self as *const () }
	}

	impl Handler for DivergingWithError {
		fn addr(&self) -> *const () { *self as *const () }
	}

	impl Handler for PageFault {
		fn addr(&self) -> *const () { *self as *const () }
	}

	impl Handler for ControlFlow {
		fn addr(&self) -> *const () { *self as *const () }
	}
}

pub mod entry {
	use core::marker::PhantomData;
	use core::num::NonZeroU8;
	use crate::hal::arch::amd64::idt::handler::Handler;

	pub enum Type {
		InterruptGate,
		InterruptTrap
	}

	#[derive(Clone, Copy)]
	#[repr(transparent)]
	struct Attributes(u8);

	impl Type {
		const fn const_u8(self) -> u8 {
			match self {
				Type::InterruptTrap => 0xF,
				Type::InterruptGate => 0xE
			}
		}
	}

	impl Attributes {
		const fn empty() -> Self { Self(0) }
		const fn new(ty: Type, dpl: u8) -> Self {
			Self(ty.const_u8() | (dpl << 5) | (1<<7))
		}
	}

	#[derive(Clone, Copy)]
	#[repr(C)]
	pub struct Entry<F> {
		pointer_low: u16,
		segment_selector: u16,
		ist: Option<NonZeroU8>,
		attributes: Attributes,
		pointer_middle: u16,
		pointer_high: u32,
		_reserved: u32,
		_phantom: PhantomData<F>
	}

	impl<F> Entry<F> {
		pub const fn empty() -> Self {
			Self {
				pointer_low: 0,
				segment_selector: 0,
				ist: None,
				attributes: Attributes::empty(),
				pointer_middle: 0,
				pointer_high: 0,
				_reserved: 0,
				_phantom: PhantomData
			}
		}
	}

	impl<F> Default for Entry<F> {
		fn default() -> Self {
			Self::empty()
		}
	}

	impl Entry<unsafe extern "C" fn()> {
		pub fn new_ptr(f: unsafe extern "C" fn(), ist_idx: Option<NonZeroU8>, dpl: u8, ty: Type) -> Self {
			let addr = f as usize;
			Self {
				pointer_low: addr as u16,
				segment_selector: 8,
				ist: ist_idx,
				attributes: Attributes::new(ty, dpl),
				pointer_middle: (addr >> 16) as u16,
				pointer_high: (addr >> 32) as u32,
				_reserved: 0,
				_phantom: PhantomData
			}
		}
	}

	impl<F: Handler> Entry<F> {
		pub fn new(f: F, ist_idx: Option<NonZeroU8>, dpl: u8, ty: Type) -> Self {
			let addr = f.addr() as usize;
			Self {
				pointer_low: addr as u16,
				segment_selector: 8,
				ist: ist_idx,
				attributes: Attributes::new(ty, dpl),
				pointer_middle: (addr >> 16) as u16,
				pointer_high: (addr >> 32) as u32,
				_reserved: 0,
				_phantom: PhantomData
			}
		}
	}
}

use entry::Entry;
use kernel_api::sync::OnceLock;

#[repr(C, align(16))]
pub struct Idt {
	pub(crate) entries: [Entry<unsafe extern "C" fn()>; 256]
}

impl Idt {
	pub const fn new() -> Self {
		Self {
			entries: [Entry::empty(); 256],
		}
	}

	pub fn load(&'static self) {
		let ptr = Pointer::new(self);

		unsafe {
			asm!("lidt [{}]", in(reg) &ptr);
		}
	}
}

#[repr(C, packed)]
pub struct Pointer {
	size: u16,
	address: &'static Idt
}

impl Pointer {
	fn new(idt: &'static Idt) -> Self {
		use core::mem::size_of;

		Self {
			size: u16::try_from(size_of::<Idt>()).unwrap() - 1,
			address: idt
		}
	}
}

impl Index<usize> for Idt {
	type Output = Entry<unsafe extern "C" fn()>;

	fn index(&self, index: usize) -> &Self::Output {
		&self.entries[index]
	}
}

impl IndexMut<usize> for Idt {
	fn index_mut(&mut self, index: usize) -> &mut Self::Output {
		&mut self.entries[index]
	}
}

pub static IDT: OnceLock<Idt> = OnceLock::new();
