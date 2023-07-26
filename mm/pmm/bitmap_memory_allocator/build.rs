use std::env;
use std::path::Path;

fn main() {
    let link_script = "../kernel/src/arch/amd64/linker_module.ld";
    let link_script = Path::new(link_script);

    let flags = ["-C", "link-args=-export-dynamic", /*"-C", "prefer-dynamic",*/ "-Z", "export-executable-symbols=on"];

    if env::var("TARGET").unwrap() == "x86_64-unknown-popcorn" {
        //println!("cargo:rustc-link-arg=-T{}", link_script.canonicalize().unwrap().display());
    }
}