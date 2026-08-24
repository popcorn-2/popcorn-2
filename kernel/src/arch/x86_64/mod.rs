mod interrupts;
mod msr;
#[doc(hidden)]
pub mod tss;
mod gdt;
mod syscall;
mod pic;

pub use interrupts::get_and_disable_interrupts;
use kernel_api::sync::LazyLock;

pub fn target_bsp_start() {
	get_and_disable_interrupts();
	percpu_v2!(arch).gdt.load();
	interrupts::IDT.load();
	syscall::init();
	pic::init();
	// enable SMAP/SMEP
	// enable and configure XSAVE if exists
}

pub struct Percpu {
	gdt: LazyLock<gdt::Gdt>,
}

impl Percpu {
	pub const fn new() -> Self {
		Self {
			gdt: gdt::Gdt::INIT,
		}
	}
}
