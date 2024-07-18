pub mod arch;
pub mod paging;
pub mod paging2;
pub mod exception;
pub mod acpi;
pub mod timing;

#[allow(unused_imports)] use crate::prelude::*;
use core::fmt::Debug;
use core::mem::MaybeUninit;
use kernel_api::memory::mapping;
use kernel_api::memory::mapping::Stack;
use kernel_api::memory::r#virtual::Global;
pub(crate) use macros::Hal;
use paging2::{KTable, TTable};
use crate::threading::{ThreadControlBlock, ThreadPointer, WakeReason, PointerView, ArgTuple};
use core::num::NonZero;
use crate::non_zero;

pub enum Result { Success, Failure }

pub trait SaveStateTr: Debug + Default {
	fn new<Args: ArgTuple>(tcb: &mut ThreadControlBlock, init: unsafe extern "C" fn(), main: extern "C" fn(Args) -> !, args: [MaybeUninit<usize>; 4]) -> Self;
}

#[repr(C)]
pub struct ContextSwitchPreserve(pub ThreadPointer, pub Option<WakeReason>);

pub unsafe trait Hal {
	type SerialOut: FormatWriter;
	type KTableTy: KTable + Send + Sync;
	type TTableTy: TTable;
	type SaveState: SaveStateTr;
	type LocalTimer: timing::Timer;

	fn breakpoint();
	fn exit(result: Result) -> !;
	fn debug_output(data: &[u8]) -> core::result::Result<(), ()>;
	fn early_init();
	fn post_acpi_init();
	fn enable_interrupts();
	fn get_and_disable_interrupts() -> usize;
	fn set_interrupts(old_state: usize);
	unsafe fn load_tls(ptr: *mut u8);
	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy);
	unsafe extern "C" fn switch_thread(from: &PointerView, to: &PointerView, preserve: ContextSwitchPreserve) -> ContextSwitchPreserve;

	const MIN_IRQ_NUM: usize;
	const MAX_IRQ_NUM: usize;
}

const _: () = if align_of::<KTableTy>() != 8 { panic!("for... reasons... KTables must be 8 byte aligned"); };

pub trait FormatWriter {
	fn print(fmt: core::fmt::Arguments);
}

pub trait InterruptTable {
	unsafe fn set_syscall_handler(handler: unsafe fn());
}

mod hal_impl {
	use super::*;

	pub type SerialOut = <arch::Arch as Hal>::SerialOut;
	pub type KTableTy = <arch::Arch as Hal>::KTableTy;
	pub type TTableTy = <arch::Arch as Hal>::TTableTy;
	pub type SaveState = <arch::Arch as Hal>::SaveState;
	pub type LocalTimer = <arch::Arch as Hal>::LocalTimer;

	pub fn breakpoint() { <arch::Arch as Hal>::breakpoint() }
	pub fn exit(result: Result) -> ! { <arch::Arch as Hal>::exit(result) }
	pub fn debug_output(data: &[u8]) -> core::result::Result<(), ()> { <arch::Arch as Hal>::debug_output(data) }
	pub fn early_init() { <arch::Arch as Hal>::early_init() }
	pub fn post_acpi_init() { <arch::Arch as Hal>::post_acpi_init() }
	#[export_name = "__popcorn_enable_irq"] pub fn enable_interrupts() { <arch::Arch as Hal>::enable_interrupts() }
	#[export_name = "__popcorn_disable_irq"] pub fn get_and_disable_interrupts() -> usize { <arch::Arch as Hal>::get_and_disable_interrupts() }
	#[export_name = "__popcorn_set_irq"] pub fn set_interrupts(old_state: usize) { <arch::Arch as Hal>::set_interrupts(old_state) }
	pub unsafe fn load_tls(ptr: *mut u8) { <arch::Arch as Hal>::load_tls(ptr) }
	pub unsafe fn construct_tables() -> (KTableTy, TTableTy) { <arch::Arch as Hal>::construct_tables() }
	pub unsafe extern "C" fn switch_thread(from: &mut ThreadControlBlock, to: &ThreadControlBlock) { <arch::Arch as Hal>::switch_thread(from, to) }

	pub const MIN_IRQ_NUM: usize = <arch::Arch as Hal>::MIN_IRQ_NUM;
	pub const MAX_IRQ_NUM: usize = <arch::Arch as Hal>::MAX_IRQ_NUM;
}
pub use hal_impl::*;

#[macro_export]
macro_rules! sprintln {
    () => { $crate::sprint!("\n") };
	($($arg:tt)*) => { $crate::sprint!("{}\n", format_args!($($arg)*)) }
}

#[macro_export]
macro_rules! sprint {
	($($arg:tt)*) => {{
		use $crate::hal::FormatWriter;
		$crate::hal::SerialOut::print(format_args!($($arg)*))
	}}
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
