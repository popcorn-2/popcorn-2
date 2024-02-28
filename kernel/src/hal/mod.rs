pub mod arch;
pub mod paging;
pub mod paging2;
pub mod exception;

use alloc::borrow::Cow;
use core::arch::asm;
use core::fmt::Debug;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::physical::highmem;
use kernel_api::memory::r#virtual::Global;
pub(crate) use macros::Hal;
use paging2::{KTable, TTable, TTableTy};
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

	const MIN_IRQ_NUM: usize;
	const MAX_IRQ_NUM: usize;
	fn set_irq_handler(handler: extern "C" fn(usize));
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
		use $crate::hal::FormatWriter;
		<$crate::hal::HalTy as $crate::hal::Hal>::SerialOut::print(format_args!($($arg)*))
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
		let save_state = SaveState::new(&mut new_thread, startup, main);
		new_thread.save_state = save_state;

		new_thread
	}
}

#[derive(Debug, PartialEq, Eq)]
pub enum ThreadState {
	Ready,
	Running
}

#[export_name = "__popcorn_enable_irq"]
fn enable_interrupts() {
	<HalTy as Hal>::enable_interrupts()
}

#[export_name = "__popcorn_disable_irq"]
fn get_and_disable_interrupts() -> usize {
	<HalTy as Hal>::get_and_disable_interrupts()
}

#[export_name = "__popcorn_set_irq"]
fn set_interrupts(old_state: usize) {
	<HalTy as Hal>::set_interrupts(old_state)
}
