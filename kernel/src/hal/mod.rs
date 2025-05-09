pub mod arch;
pub mod paging;
pub mod paging2;
pub mod exception;
pub mod acpi;
pub mod timing;
pub mod interrupts_v2;

#[allow(unused_imports)] use crate::prelude::*;
use core::fmt::Debug;
pub(crate) use macros::Hal;
use paging2::{KTable, TTable};
use crate::threading::{ThreadControlBlock, ThreadPointer, WakeReason, PointerView};
use crate::hal::interrupts_v2::Vector;
use kernel_api::memory::VirtualAddress;

pub enum Result { Success, Failure }

pub trait SaveStateTr: Debug + Default {
	fn new(tcb: &mut ThreadControlBlock, init: unsafe extern "C" fn(), main: extern "C" fn(usize) -> !, args: usize) -> Self;
	/// # Safety
	/// 
	/// The thread must never have been run
	unsafe fn set_entry(&mut self, main: extern "C" fn(usize) -> !, args: usize);
}

#[repr(C)]
pub struct ContextSwitchPreserve(pub ThreadPointer, pub Option<WakeReason>);

pub unsafe trait Hal {
	type SerialOut: FormatWriter;
	type KTableTy: KTable + Send + Sync;
	type TTableTy: TTable;
	type SaveState: SaveStateTr;

	fn breakpoint();
	fn exit(result: Result) -> !;
	fn debug_output(data: &[u8]) -> core::result::Result<(), ()>;
	fn early_init();
	fn post_acpi_init();
	fn enable_interrupts();
	fn get_and_disable_interrupts() -> usize;
	fn set_interrupts(old_state: usize);
	unsafe fn load_tls(ptr: *mut u8);
	unsafe fn load_user_tls(ptr: *mut u8);
	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy);
	unsafe extern "C" fn switch_thread(from: &mut PointerView, to: &mut PointerView, preserve: ContextSwitchPreserve) -> ContextSwitchPreserve;

	fn send_ipi(target: IpiTarget) -> ::core::result::Result<(), ()>;
	fn send_local_eoi(vector: Vector);
	fn wait_for_interrupt();
	fn first_thread_init(tcb: &ThreadControlBlock);
	extern "C" fn switch_to_userspace_at(addr: VirtualAddress, stack_top: VirtualAddress) -> !;

	const IPI_VECTOR: Vector;
	const SPURIOUS_VECTOR: Vector;
}

// Alignment of the extern type version must be known, so ensure it's the same value here
const _: () = if align_of::<KTableTy>() != 8 { panic!("for... reasons... KTables must be 8 byte aligned"); };

pub enum IpiTarget {
	SelfIpi,
}

pub trait FormatWriter {
	fn print(fmt: core::fmt::Arguments);
	fn read() -> u8;
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
	#[inline] pub unsafe fn load_user_tls(ptr: *mut u8) { <arch::Arch as Hal>::load_user_tls(ptr) }
	#[inline] pub unsafe extern "C" fn switch_thread(from: &mut PointerView, to: &mut PointerView, preserve: ContextSwitchPreserve) -> ContextSwitchPreserve { <arch::Arch as Hal>::switch_thread(from, to, preserve) }

	pub fn send_ipi(target: IpiTarget) -> ::core::result::Result<(), ()> { <arch::Arch as Hal>::send_ipi(target) }
	
	pub fn send_local_eoi(vector: Vector) { <arch::Arch as Hal>::send_local_eoi(vector) }
	pub fn wait_for_interrupt() { <arch::Arch as Hal>::wait_for_interrupt() }
	#[inline] pub fn first_thread_init(tcb: &ThreadControlBlock) { <arch::Arch as Hal>::first_thread_init(tcb) }
	#[inline] pub fn switch_to_userspace_at(addr: VirtualAddress, stack_top: VirtualAddress) -> ! { <arch::Arch as Hal>::switch_to_userspace_at(addr, stack_top) }

	pub const IPI_VECTOR: Vector = <arch::Arch as Hal>::IPI_VECTOR;
	pub const SPURIOUS_VECTOR: Vector = <arch::Arch as Hal>::SPURIOUS_VECTOR;
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
