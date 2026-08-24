mod interrupts;
mod msr;
#[doc(hidden)]
pub mod tss;
mod gdt;
mod syscall;
mod pic;

pub use interrupts::get_and_disable_interrupts;

pub fn target_bsp_start() {
	get_and_disable_interrupts();
	gdt::BSP_GDT.load();
	interrupts::IDT.load();
	syscall::init();
	pic::init();
	// enable SMAP/SMEP
	// enable and configure XSAVE if exists
}

pub struct Percpu {}

impl Percpu {
	pub const fn new() -> Self {
		Self {}
	}
}
