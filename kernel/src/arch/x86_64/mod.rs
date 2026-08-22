mod interrupts;
mod msr;
#[doc(hidden)]
pub mod tss;
mod gdt;

pub use interrupts::get_and_disable_interrupts;

pub fn target_bsp_start() {
	get_and_disable_interrupts();
	gdt::BSP_GDT.load();
	// configure STAR, LSTAR, SFMASK
	// load IDT
	// initialise PIC
	// enable SMAP/SMEP
	// enable and configure XSAVE if exists
}
