pub mod arch;
pub mod paging;
pub mod paging2;
pub mod exception;
pub mod acpi;
pub mod interrupts_v2;

use core::fmt::Debug;
use paging2::{KTable, TTable};
use crate::hal::interrupts_v2::Vector;
use kernel_api::address_space::Kernel;
use kernel_api::mapping::{Mapping, Stack};

#[expect(unused)] pub enum Result { Success, Failure }

pub trait SaveStateTr: Debug + Default {
	fn new(tcb: &mut Mapping<Stack, Kernel>, main: extern "C" fn(usize) -> !, args: usize) -> core::result::Result<Self, AllocError>;
}

pub unsafe trait Hal {
	type SerialOut: FormatWriter;
	type KTableTy: KTable + Send + Sync;
	type TTableTy: TTable;

	fn breakpoint();
	fn exit(result: Result) -> !;
	fn debug_output(data: &[u8]) -> core::result::Result<(), ()>;
	fn enable_interrupts();
	fn get_and_disable_interrupts() -> usize;
	fn set_interrupts(old_state: usize);
	unsafe fn load_tls(ptr: *mut u8);
	fn load_user_tls(ptr: *mut u8);

	fn send_ipi(target: IpiTarget) -> core::result::Result<(), ()>;
	fn send_local_eoi(vector: Vector);
	fn wait_for_interrupt();

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

mod hal_impl {
	use super::*;

	pub type SerialOut = <arch::Arch as Hal>::SerialOut;
	pub type KTableTy = <arch::Arch as Hal>::KTableTy;
	pub type TTableTy = <arch::Arch as Hal>::TTableTy;

	#[inline] #[expect(unused)] pub fn breakpoint() { <arch::Arch as Hal>::breakpoint() }
	#[inline] #[expect(unused)] pub fn exit(result: Result) -> ! { <arch::Arch as Hal>::exit(result) }
	#[inline] #[expect(unused)] pub fn debug_output(data: &[u8]) -> core::result::Result<(), ()> { <arch::Arch as Hal>::debug_output(data) }
	pub fn enable_interrupts() { <arch::Arch as Hal>::enable_interrupts() }
	pub fn get_and_disable_interrupts() -> usize { <arch::Arch as Hal>::get_and_disable_interrupts() }
	pub fn set_interrupts(old_state: usize) { <arch::Arch as Hal>::set_interrupts(old_state) }
	#[inline] pub unsafe fn load_tls(ptr: *mut u8) { unsafe { <arch::Arch as Hal>::load_tls(ptr) } }
	#[inline] pub fn load_user_tls(ptr: *mut u8) { <arch::Arch as Hal>::load_user_tls(ptr) }

	#[inline] pub fn send_ipi(target: IpiTarget) -> core::result::Result<(), ()> { <arch::Arch as Hal>::send_ipi(target) }

	#[inline] pub fn send_local_eoi(vector: Vector) { <arch::Arch as Hal>::send_local_eoi(vector) }
	#[inline] pub fn wait_for_interrupt() { <arch::Arch as Hal>::wait_for_interrupt() }

	pub const IPI_VECTOR: Vector = <arch::Arch as Hal>::IPI_VECTOR;
	pub const SPURIOUS_VECTOR: Vector = <arch::Arch as Hal>::SPURIOUS_VECTOR;
}
pub use hal_impl::*;
use kernel_api::allocator::AllocError;

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
