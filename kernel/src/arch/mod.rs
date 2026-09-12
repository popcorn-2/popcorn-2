cfg_select! {
	target_arch = "x86_64" => {
		#[doc(hidden)]
		pub mod x86_64;
		use x86_64 as imp_arch;

		mod acpi;
		use acpi as imp_platform;
	}
}

mod common;

pub use imp_arch::{
	SavedRegisters,
	Percpu,
	target_bsp_start,
	get_and_disable_interrupts,
	switch_to_userspace,
};

pub use imp_platform::{
	post_memory_init,
};
