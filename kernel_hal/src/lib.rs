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
#![feature(kernel_virtual_memory)]

//#![warn(missing_docs)]

extern crate alloc;

pub mod arch;

pub mod paging;

pub mod paging2;

use alloc::borrow::Cow;
use core::fmt::Debug;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::highmem;
use kernel_api::memory::r#virtual::Global;
pub(crate) use macros::Hal;
use crate::paging2::{KTable, TTable, TTableTy};
use core::num::NonZeroUsize;

pub enum Result { Success, Failure }

pub trait SaveState: Debug + Default {
	fn new(tcb: &mut ThreadControlBlock, init: fn(), main: fn() -> !) -> Self;
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
	fn get_and_disable_interrupts() -> usize;
	fn set_interrupts(old_state: usize);
	unsafe fn load_tls(ptr: *mut u8);
	//fn interrupt_table() -> impl InterruptTable;
	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy);
	unsafe extern "C" fn switch_thread(from: &mut ThreadControlBlock, to: &ThreadControlBlock);
}

const _: () = { if core::mem::align_of::<<HalTy as Hal>::KTableTy>() != 8 { panic!("for... reasons... KTables must be 8 byte aligned"); } };

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
	pub name: Cow<'static, str>,
	pub kernel_stack: Stack<'static, Global>,
	pub state: ThreadState,
}

impl ThreadControlBlock {
	pub fn new(name: Cow<'static, str>, ttable: TTableTy, startup: fn(), main: fn() -> !) -> Self {
		let new_stack = Stack::new(
			mapping::Config::<Global>::new(NonZeroUsize::new(8).unwrap())
		).unwrap();

		let mut new_thread = ThreadControlBlock {
			ttable,
			save_state: Default::default(),
			name,
			kernel_stack: new_stack,
			state: ThreadState::Ready,
		};
		let save_state = <HalTy as Hal>::SaveState::new(&mut new_thread, startup, main);
		new_thread.save_state = save_state;

		new_thread
	}
}

#[derive(Debug, PartialEq, Eq)]
pub enum ThreadState {
	Ready,
	Running
}
