cfg_select! {
	target_arch = "x86_64" => {
		mod x86_64;
		use x86_64 as imp_arch;

		mod acpi;
		use acpi as imp_platform;
	}
}

mod common;
