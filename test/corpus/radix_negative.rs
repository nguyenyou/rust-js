// `{:x}`, `{:b}` and `{:o}` of a negative number show its bits, as wide as
// its type: `-1i8` is `ff`, and `-1i64` sixteen `f`s. Found by seed 39.
fn show(a: i8, b: i16, c: i32, d: i64) {
    println!("{:x} {:x} {:x} {:x}", a, b, c, d);
    println!("{:#x} {:#X} {:b} {:o}", d, d, d, d);
    println!("{:x} {:#x}", i64::MIN, -2147483648i64);
}

fn main() {
    show(-1, -2, -3, -4);
    show(i8::MIN, i16::MIN, i32::MIN, i64::MIN);
}
