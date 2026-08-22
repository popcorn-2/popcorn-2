mod msr;
#[doc(hidden)]
pub mod tss;
mod gdt;

pub fn target_bsp_start() {
	// disable IRQ
	gdt::BSP_GDT.load();
	// configure STAR, LSTAR, SFMASK
	// load IDT
	// initialise PIC
	// enable SMAP/SMEP
	// enable and configure XSAVE if exists
}
