use std::env;
use std::io;
use std::fs;
use std::path::PathBuf;
use std::fmt::Write;

const PIP_SOURCES: &[&str] = &["io.pip", "proc.pip", "server.pip", "mem.pip"];

fn main() -> io::Result<()> {
    let src_dir = {
        let mut buf = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        buf.push("../core-protocols/src/core");
        buf
    };

	let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
	let mut buf = String::from("pub mod core {\nuse super::{std, popcorn_server, StrExt};");

	for name in PIP_SOURCES {
		let file = src_dir.join(name);

		pipc::process_file(&file, pipc::OutputFormat::Rust, pipc::OutputTy::Server, &out_dir)
                .map_err(std::io::Error::other)?;

		let _ = writeln!(&mut buf, "pub mod {name} {{ use super::{{std, popcorn_server, StrExt}}; include!(\"{}/{name}.rs\"); }}", out_dir.display(), name = file.file_stem().unwrap().display());
	}

	buf += "}\n";

    fs::write(
		out_dir.join("protocol.gen.rs"),
		buf,
	)?;

    Ok(())
}
