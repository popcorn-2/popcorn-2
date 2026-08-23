mod interrupts;
mod msr;
#[doc(hidden)]
pub mod tss;
mod gdt;

pub use interrupts::get_and_disable_interrupts;

pub fn target_bsp_start() {
	get_and_disable_interrupts();
	gdt::BSP_GDT.load();
	interrupts::IDT.load();
	// configure STAR, LSTAR, SFMASK
	// initialise PIC
	// enable SMAP/SMEP
	// enable and configure XSAVE if exists
}
