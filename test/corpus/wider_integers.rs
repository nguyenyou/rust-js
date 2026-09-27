//@ compile-fail: does not support
// 128-bit integers have no JS representation yet (ADR 0086).
fn main() {
    let x: u128 = 1 << 100;
    println!("{x}");
}
