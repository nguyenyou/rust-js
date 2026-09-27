// An enum's discriminant, cast to an integer as wide as it, is exact: past
// 2^53, where a JS number rounds. The enum comes through an argument, so the
// cast isn't worked out by rustc. Found in review.
#[derive(Clone, Copy)]
#[repr(u64)]
enum Big {
    Small = 3,
    Past = 9007199254740993,
    Max = 18446744073709551615,
}

#[derive(Clone, Copy)]
#[repr(i64)]
enum Signed {
    Low = -9223372036854775808,
    Odd = -9007199254740993,
    Zero = 0,
}

fn big(b: Big) -> u64 {
    b as u64
}

fn signed(s: Signed) -> i64 {
    s as i64
}

fn main() {
    for b in [Big::Small, Big::Past, Big::Max] {
        println!("{} {}", big(b), big(b) as u8);
    }
    for s in [Signed::Low, Signed::Odd, Signed::Zero] {
        println!("{} {}", signed(s), signed(s) as i32);
    }
}
