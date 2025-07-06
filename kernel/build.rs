use std::{env, fs};
use std::path::PathBuf;
use std::process::Command;

fn main() {
	println!("cargo:rerun-if-changed=/Users/Eliyahu/popcorn2/pipc/target/debug/pipc");
	println!("cargo:rerun-if-changed=/Users/Eliyahu/popcorn2/pipc/test_input/core.pip");

	let output = Command::new("/Users/Eliyahu/popcorn2/pipc/target/debug/pipc")
			.arg("/Users/Eliyahu/popcorn2/pipc/test_input/core.pip")
			.output()
			.unwrap();
	let mut f = PathBuf::from(env::var("OUT_DIR").unwrap());
	f.push("protocol.gen.rs");
	println!("cargo:rerun-if-changed={}", f.display());
	fs::write(f, output.stdout).unwrap();
}
