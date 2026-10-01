// A bitwise operator in generic code, `a | b` of a `T: BitOr`, is its
// dictionary's, as `a + b` of a `T: Add` is (ADR 0108): a number's or a
// `bool`'s is its own `|`, and a type of the crate's own its impl's
// `bitor`. Of a number and a reference to one, `a & &b`, it's the number's.
use std::ops::{BitAnd, BitOr, BitXor, Shl, Shr};

fn either<T: BitOr<Output = T>>(a: T, b: T) -> T {
    a | b
}

fn masked<T: BitAnd<Output = T> + BitXor<Output = T> + Copy>(value: T, mask: T) -> (T, T) {
    (value & mask, value ^ mask)
}

// A shift's amount of another type than the value's, as `x << 3u32` is.
fn shifted<T: Shl<u32, Output = T> + Shr<u32, Output = T> + Copy>(value: T, by: u32) -> (T, T) {
    (value << by, value >> by)
}

// By a `u64`, a BigInt, of a narrower number: the amount a JS number.
fn shifted_far<T: Shl<u64, Output = T>>(value: T, by: u64) -> T {
    value << by
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Flags(u8);

impl BitOr for Flags {
    type Output = Flags;
    fn bitor(self, other: Flags) -> Flags {
        Flags(self.0 | other.0)
    }
}

fn main() {
    println!("{} {} {:?}", either(0b0101u8, 0b0011u8), either(false, true), either(Flags(1), Flags(4)));
    println!("{:?} {:?}", masked(0xF0F0u32, 0xFF00u32), masked(0xF0F0_0000_0000u64, 0xFFFF_0000_0000u64));
    println!("{:?} {:?}", masked(-6i32, 3i32), masked(true, false));
    // A `u8` wraps, an `i32`'s `>>` keeps its sign, and a `u64`'s is a BigInt's.
    println!("{:?} {:?} {:?}", shifted(0b1100_0001u8, 2), shifted(-64i32, 3), shifted(1u64 << 40, 4));
    println!("{} {}", shifted_far(3u32, 4), shifted_far(1u16, 17));

    // Of references to numbers, as an iterator gives them.
    let a = 0b1100u32;
    let b = &0b1010u32;
    println!("{} {} {} {}", a & b, &a | b, a ^ *b, a << &2u8);
    let mut c = 0xFFu32;
    c &= b;
    c |= &0x100;
    c ^= &1;
    c >>= &1u8;
    c <<= &3i64;
    let far = 33i64;
    println!("{} {}", c, a << &far);
    let total = [1u8, 2, 4].iter().fold(0u8, |all, bit| all | bit);
    println!("{}", total);
}
