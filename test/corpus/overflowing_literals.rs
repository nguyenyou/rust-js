// A literal too big for its type wraps to it, as rustc does where
// `overflowing_literals` is allowed: `256u8` is 0, `-129i8` is 127.
#[allow(overflowing_literals)]
fn main() {
    let a: u8 = 256;
    let b: i8 = 200;
    let c = -129i8;
    let d: u16 = 70000;
    let e = -32769i16;
    let f: i16 = 0xffff;
    println!("{a} {b} {c} {d} {e} {f}");
    println!("{} {}", -129i8 as u8, 300u16 as u8);
}
