use core::arch::asm;
use core::marker::{FnPtr, PhantomData};
use core::ops::{Deref, Index, IndexMut};

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

	pub trait Handler {}
	impl Handler for Normal {}
	impl Handler for NormalWithError {}
	impl Handler for Diverging {}
	impl Handler for DivergingWithError {}
	impl Handler for PageFault {}
	impl Handler for ControlFlow {}
}

pub mod entry {
	use core::marker::{FnPtr, PhantomData};
	use core::num::NonZeroU8;
	use crate::idt::handler::Handler;

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
	pub struct Entry<F: Handler + FnPtr> {
		pointer_low: u16,
		segment_selector: u16,
		ist: Option<NonZeroU8>,
		attributes: Attributes,
		pointer_middle: u16,
		pointer_high: u32,
		_reserved: u32,
		_phantom: PhantomData<F>
	}

	impl<F: Handler + FnPtr> Entry<F> {
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

	impl<F: Handler + FnPtr> Default for Entry<F> {
		fn default() -> Self {
			Self::empty()
		}
	}
}

use entry::Entry;
use kernel_api::sync::{Mutex, OnceLock};

#[repr(C, align(16))]
pub struct Idt<const ENTRY_COUNT: usize> where [(); ENTRY_COUNT - 32]: {
	pub div_by_zero: Entry<handler::Normal>,
	pub debug: Entry<handler::Normal>,
	pub nmi: Entry<handler::Normal>,
	pub breakpoint: Entry<handler::Normal>,
	pub overflow: Entry<handler::Normal>,
	pub bound_range_fault: Entry<handler::Normal>,
	pub invalid_opcode: Entry<handler::Normal>,
	pub device_not_available: Entry<handler::Normal>,
	pub double_fault: Entry<handler::Diverging>,
	reserved1: Entry<handler::Normal>,
	pub invalid_tss: Entry<handler::NormalWithError>,
	pub segment_not_present: Entry<handler::NormalWithError>,
	pub stack_exception: Entry<handler::NormalWithError>,
	pub general_protection_fault: Entry<handler::NormalWithError>,
	pub page_fault: Entry<handler::PageFault>,
	reserved2: Entry<handler::Normal>,
	pub x87_floating_point: Entry<handler::Normal>,
	pub alignment_check: Entry<handler::NormalWithError>,
	pub machine_check: Entry<handler::Diverging>,
	pub sse_floating_point: Entry<handler::Normal>,
	reserved3: Entry<handler::Normal>,
	pub control_protection: Entry<handler::ControlFlow>,
	reserved4: [Entry<handler::Normal>; 6],
	pub hypervisor: Entry<handler::Normal>,
	pub virtualization: Entry<handler::Normal>,
	pub security: Entry<handler::Normal>,
	reserved: Entry<handler::Normal>,
	other: [Entry<handler::Normal>; ENTRY_COUNT - 32]
}

impl<const ENTRY_COUNT: usize> Idt<ENTRY_COUNT> where [(); ENTRY_COUNT - 32]: {
	pub const fn new() -> Self {
		Self {
			// have to do this because Default isn't const and isn't implemented for generic arrays
			div_by_zero: Entry::empty(),
			debug: Entry::empty(),
			nmi: Entry::empty(),
			breakpoint: Entry::empty(),
			overflow: Entry::empty(),
			bound_range_fault: Entry::empty(),
			invalid_opcode: Entry::empty(),
			device_not_available: Entry::empty(),
			double_fault: Entry::empty(),
			reserved1: Entry::empty(),
			invalid_tss: Entry::empty(),
			segment_not_present: Entry::empty(),
			stack_exception: Entry::empty(),
			general_protection_fault: Entry::empty(),
			page_fault: Entry::empty(),
			reserved2: Entry::empty(),
			x87_floating_point: Entry::empty(),
			alignment_check: Entry::empty(),
			machine_check: Entry::empty(),
			sse_floating_point: Entry::empty(),
			reserved3: Entry::empty(),
			control_protection: Entry::empty(),
			reserved4: [Entry::empty(); 6],
			hypervisor: Entry::empty(),
			virtualization: Entry::empty(),
			security: Entry::empty(),
			reserved: Entry::empty(),
			other: [Entry::empty(); ENTRY_COUNT - 32],
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
pub struct Pointer<const ENTRY_COUNT: usize> where [(); ENTRY_COUNT - 32]: {
	size: u16,
	address: &'static Idt<ENTRY_COUNT>
}

impl<const ENTRY_COUNT: usize> Pointer<ENTRY_COUNT> where [(); ENTRY_COUNT - 32]: {
	fn new(idt: &'static Idt<ENTRY_COUNT>) -> Self {
		use core::mem::size_of;

		Self {
			size: u16::try_from(size_of::<Idt<ENTRY_COUNT>>()).unwrap() - 1,
			address: idt
		}
	}
}

impl<const ENTRY_COUNT: usize> Index<usize> for Idt<ENTRY_COUNT> where [(); ENTRY_COUNT - 32]: {
	type Output = Entry<handler::Normal>;

	fn index(&self, index: usize) -> &Self::Output {
		match index {
			0..=31 => todo!(),
			i @ 32.. => &self.other[i - 32]
		}
	}
}

impl<const ENTRY_COUNT: usize> IndexMut<usize> for Idt<ENTRY_COUNT> where [(); ENTRY_COUNT - 32]: {
	fn index_mut(&mut self, index: usize) -> &mut Self::Output {
		match index {
			0..=31 => todo!(),
			i @ 32.. => &mut self.other[i - 32]
		}
	}
}

const ENTRY_COUNT: usize = 32;

pub static IDT: OnceLock<Idt<ENTRY_COUNT>> = OnceLock::new();

pub trait ExceptionTable {}


impl<const ENTRY_COUNT: usize> ExceptionTable for Idt<ENTRY_COUNT> where [(); ENTRY_COUNT - 32]: {

}
