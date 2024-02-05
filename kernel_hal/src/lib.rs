#![no_std]

#![feature(type_alias_impl_trait)]
#![feature(abi_x86_interrupt)]
#![feature(generic_const_exprs)]
#![feature(offset_of)]
#![feature(asm_const)]
#![feature(const_mut_refs)]
#![feature(min_specialization)]
#![feature(pointer_is_aligned)]
#![feature(naked_functions)]

#![feature(kernel_sync_once)]
#![feature(kernel_physical_page_offset)]
#![feature(kernel_memory_addr_access)]
#![feature(kernel_internals)]
#![feature(kernel_mmap)]

//#![warn(missing_docs)]

pub mod arch;

pub mod paging;

pub mod paging2;

use core::fmt::Debug;
use kernel_api::memory::mapping::Stack;
pub(crate) use macros::Hal;
use crate::paging2::{KTable, TTable, TTableTy};

pub enum Result { Success, Failure }

pub trait SaveState: Debug + Default {
	fn new(tcb: &mut ThreadControlBlock, ret: usize) -> Self;
}

pub unsafe trait Hal {
	type SerialOut: FormatWriter;
	type KTableTy: KTable;
	type TTableTy: TTable<KTableTy = Self::KTableTy>;
	type SaveState: SaveState;

	fn breakpoint();
	fn exit(result: Result) -> !;
	fn debug_output(data: &[u8]) -> core::result::Result<(), ()>;
	fn early_init();
	fn init_idt();
	fn enable_interrupts();
	fn get_and_disable_interrupts() -> bool;
	unsafe fn load_tls(ptr: *mut u8);
	//fn interrupt_table() -> impl InterruptTable;
	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy);
	unsafe extern "C" fn switch_thread(from: &mut ThreadControlBlock, to: &ThreadControlBlock);
}

pub trait FormatWriter {
	fn print(fmt: core::fmt::Arguments);
}

pub trait InterruptTable {
	unsafe fn set_syscall_handler(handler: unsafe fn());
}

pub type HalTy = impl Hal;

#[macro_export]
macro_rules! sprintln {
    () => { $crate::sprint!("\n") };
	($($arg:tt)*) => { $crate::sprint!("{}\n", format_args!($($arg)*)) }
}

#[macro_export]
macro_rules! sprint {
	($($arg:tt)*) => {{
		use $crate::FormatWriter;
		<$crate::HalTy as $crate::Hal>::SerialOut::print(format_args!($($arg)*))
	}}
}

#[derive(Debug)]
pub struct ThreadControlBlock {
	pub ttable: TTableTy,
	pub save_state: <HalTy as Hal>::SaveState,
	pub name: &'static str,
	pub kernel_stack: Stack<'static>,
	pub state: ThreadState,
}

#[derive(Debug)]
pub enum ThreadState {
	Ready,
	Running
}
