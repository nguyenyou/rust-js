//@ compile-fail: rust-js does not support `&mut` references to a `static mut` yet
// A `&mut` to a `static mut` is rejected (ADR 0096): a `&mut` of a number
// has no JS value yet. A shared one is the value (static_mut_shared.rs).
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
