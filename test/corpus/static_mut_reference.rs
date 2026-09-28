//@ compile-fail: rust-js does not support references to a `static mut` yet
// A reference to a `static mut` is rejected (ADR 0096): Rust 2024 denies
// one by default, and a `&mut` of a number has no JS value yet.
static mut TOTAL: u32 = 0;

fn add(total: &mut u32, n: u32) {
    *total += n;
}

#[allow(static_mut_refs)]
fn main() {
    unsafe {
        add(&mut TOTAL, 3);
        println!("{}", TOTAL);
    }
}
