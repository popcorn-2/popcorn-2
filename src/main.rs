#![feature(custom_test_frameworks)]
#![test_runner(tests::test_runner)]

use std::{env, fs};

fn main() {
	let current_exe = env::current_exe().unwrap();
	let disk_image_target = current_exe.with_file_name("popcorn.iso");

	fs::copy(env!("ISO_IMAGE"), &disk_image_target).unwrap();

	println!("live iso at {}", disk_image_target.display());
}

#[cfg(test)]
mod tests {
	use std::process::Command;

	// We take a useless arg here to satisfy rustc but actually just run the tests via qemu
	pub fn test_runner(_: &[()]) {
		println!("hello!");
		println!(concat!("live iso at ", env!("ISO_IMAGE")));

		let mut command = Command::new("qemu-system-x86_64");
		command.args([
			"-drive", "if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd",
			"-drive", "if=pflash,format=raw,file=OVMF_VARS.fd",
			"--no-reboot",
			"-serial", "stdio",
			//"-display", "none",
			"-drive", concat!("format=raw,file=", env!("ISO_IMAGE")),
			"-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"
		]);
		let mut child = command.spawn().unwrap();
		let status = child.wait().unwrap();

		if status.code() != Some(33) {
			std::process::exit(1);
		}
	}
}
