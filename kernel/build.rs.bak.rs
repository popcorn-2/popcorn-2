#![feature(type_ascription)]

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
	println!("cargo:rerun-if-changed=Cargo.lock");

	let link_script = "src/arch/amd64/linker.ld";
	let link_script = Path::new(link_script);

	let out_dir = env::var("OUT_DIR").unwrap();

	//nasm(&out_dir);
	//fonts(&out_dir);

	//println!("cargo:rustc-link-arg=--dynamic-linker=\"\"");
	println!("cargo:rustc-link-arg=-T{}", link_script.canonicalize().unwrap().display());
	println!("cargo:rustc-link-arg=-export-dynamic");
	println!("cargo:rustc-flags=dfirjuei -Z export-executable-symbols=on -C relocation-model=static");
}

fn nasm(out_dir: &str) {
	let asm_files = [
		"src/arch/amd64/asm/bootstrap.asm",
		"src/arch/amd64/asm/header.asm",
		"src/arch/amd64/asm/initial_mem_map.asm",
		"src/arch/amd64/asm/long_mode.asm",
		"src/arch/amd64/asm/multiboot_info.asm",
		"src/arch/amd64/asm/parse_psf.asm",
		"src/arch/amd64/asm/psf_copychar.asm",
		"src/arch/amd64/asm/termio.asm",
	];
	let nasm_args = ["-I", "src/arch/amd64/asm"];

	for file in asm_files {
		println!("cargo:rerun-if-changed={file}");

		let output_file = Path::new(file).file_name().unwrap().to_str().unwrap();
		let output_file = format!("{out_dir}/{output_file}.o");
		let default_args = [file, "-o", &output_file, "-f", "elf64", "-F", "dwarf", "-g"];
		let args = default_args.iter()
		                       .chain(nasm_args.iter());
		let status = Command::new("nasm")
				.args(args)
				.output()
				.unwrap();
		if !status.status.success() { panic!("Failed to compile {file}:\n{}", String::from_utf8_lossy(&status.stderr)) }

		println!("cargo:rustc-link-arg={output_file}");
	}
}

fn fonts(out_dir: &str) {
	let font_files = ["src/fonts/font.psf"];

	for file in font_files {
		let output_file = Path::new(file).file_name().unwrap().to_str().unwrap();
		let output_file = format!("{out_dir}/{output_file}.o");
		let args = ["-O", "elf64-x86-64", "-I", "binary", "--prefix-alloc-sections=.font", "--rename-section", ".data=.rodata", file, &output_file];
		let status = Command::new("llvm-objcopy")
				.args(args)
				.output()
				.unwrap();
		if !status.status.success() { panic!("Failed to objcopy {file}:\n{}", String::from_utf8_lossy(&status.stderr)) }

		println!("cargo:rustc-link-arg={output_file}");
	}
}
