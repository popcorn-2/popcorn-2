#![no_std]

#![feature(type_alias_impl_trait)]
#![feature(abi_x86_interrupt)]
#![feature(generic_const_exprs)]
#![feature(offset_of)]
#![feature(asm_const)]
#![feature(const_mut_refs)]
#![feature(min_specialization)]
#![feature(pointer_is_aligned)]

#![feature(kernel_sync_once)]
#![feature(kernel_physical_page_offset)]
#![feature(kernel_memory_addr_access)]
#![feature(kernel_internals)]

#![warn(missing_docs)]

pub mod arch;

pub mod paging;

pub enum Result { Success, Failure }

pub unsafe trait Hal {
	type SerialOut: FormatWriter;

	fn breakpoint();
	fn exit(result: Result) -> !;
	fn debug_output(data: &[u8]) -> core::result::Result<(), ()>;
	fn early_init();
	fn init_idt();
	fn enable_interrupts();
	fn get_and_disable_interrupts() -> bool;
	unsafe fn load_tls(ptr: *mut u8);
}

pub trait FormatWriter {
	fn print(fmt: core::fmt::Arguments);
}

pub type CurrentHal = impl Hal;

#[macro_export]
macro_rules! sprintln {
    () => { $crate::sprint!("\n") };
	($($arg:tt)*) => { $crate::sprint!("{}\n", format_args!($($arg)*)) }
}

#[macro_export]
macro_rules! sprint {
	($($arg:tt)*) => {{
		use $crate::FormatWriter;
		<$crate::CurrentHal as $crate::Hal>::SerialOut::print(format_args!($($arg)*))
	}}
}

pub(crate) use macros::Hal;
