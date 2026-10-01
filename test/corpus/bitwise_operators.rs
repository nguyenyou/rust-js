// A type of the crate's own with the bitwise operators, as a set of flags
// has them: `a | b` is its `bitor`, `a <<= 1u8` its `shl_assign`, called as
// `+` and `+=` are (ADR 0064).
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, ShlAssign, Shr, ShrAssign};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Flags(u8);

const READ: Flags = Flags(0b001);
const WRITE: Flags = Flags(0b010);
const RUN: Flags = Flags(0b100);

impl BitOr for Flags {
    type Output = Flags;
    fn bitor(self, other: Flags) -> Flags {
        Flags(self.0 | other.0)
    }
}

impl BitAnd for Flags {
    type Output = Flags;
    fn bitand(self, other: Flags) -> Flags {
        Flags(self.0 & other.0)
    }
}

impl BitXor for Flags {
    type Output = Flags;
    fn bitxor(self, other: Flags) -> Flags {
        Flags(self.0 ^ other.0)
    }
}

impl Not for Flags {
    type Output = Flags;
    fn not(self) -> Flags {
        Flags(!self.0 & 0b111)
    }
}

// A shift by another integer type than its own, as `x << 1u8` is.
impl Shl<u8> for Flags {
    type Output = Flags;
    fn shl(self, by: u8) -> Flags {
        Flags((self.0 << by) & 0b111)
    }
}

impl Shr<u32> for Flags {
    type Output = Flags;
    fn shr(self, by: u32) -> Flags {
        Flags(self.0 >> by)
    }
}

impl BitOrAssign for Flags {
    fn bitor_assign(&mut self, other: Flags) {
        self.0 |= other.0;
    }
}

impl BitAndAssign for Flags {
    fn bitand_assign(&mut self, other: Flags) {
        self.0 &= other.0;
    }
}

impl BitXorAssign for Flags {
    fn bitxor_assign(&mut self, other: Flags) {
        self.0 ^= other.0;
    }
}

impl ShlAssign<u8> for Flags {
    fn shl_assign(&mut self, by: u8) {
        self.0 = (self.0 << by) & 0b111;
    }
}

impl ShrAssign<u32> for Flags {
    fn shr_assign(&mut self, by: u32) {
        self.0 >>= by;
    }
}

// Of references too: `&a | &b`, without copying either.
impl<'a> BitOr<&'a Flags> for &'a Flags {
    type Output = Flags;
    fn bitor(self, other: &Flags) -> Flags {
        Flags(self.0 | other.0)
    }
}

impl Flags {
    fn has(self, other: Flags) -> bool {
        self & other == other
    }
}

fn main() {
    let rw = READ | WRITE;
    println!("{:?} {:?} {:?}", rw, rw & WRITE, rw ^ RUN);
    println!("{} {}", rw.has(WRITE), rw.has(RUN));
    println!("{:?} {:?} {:?}", !rw, READ << 2, RUN >> 1);
    println!("{:?}", &READ | &RUN);

    let mut f = READ;
    f |= RUN;
    println!("{:?}", f);
    f &= RUN;
    println!("{:?}", f);
    f ^= WRITE;
    println!("{:?}", f);
    f >>= 1;
    println!("{:?}", f);
    f <<= 2;
    println!("{:?}", f);

    // An element's, and a field's, as a place `|=` writes.
    let mut all = [READ, WRITE];
    all[1] |= RUN;
    let mut user = (READ, "ann");
    user.0 |= WRITE;
    println!("{:?} {:?} {}", all, user.0, user.1);
}
