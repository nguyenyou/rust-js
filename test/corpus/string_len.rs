//@ compile-fail: Rust counts its UTF-8 bytes, and JS its UTF-16 units
// A string's `len()` is its UTF-8 bytes, which a JS string doesn't count
// (ADR 0063): rejected rather than answered differently.
fn main() {
    let s = "é";
    println!("{}", s.len());
}
