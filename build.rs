//! Bake the toolchain's `lib` directory into the binary's rpath, so
//! `target/debug/rust-js` can find `librustc_driver` without extra env vars.

use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let out = Command::new(rustc).arg("--print=sysroot").output().expect("run rustc");
    let sysroot = String::from_utf8(out.stdout).expect("utf-8 sysroot");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}/lib", sysroot.trim());
}
