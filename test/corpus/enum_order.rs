// A derived `PartialOrd` of a fieldless enum orders by discriminant, which
// may not be the order of declaration, and may be negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    High = 2,
    Below = -1,
    Low = 1,
}

#[derive(Debug, PartialEq, PartialOrd)]
#[repr(u8)]
enum Byte {
    Two = 2,
    Max = !0 as u8,
    One = 1,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Plain {
    First,
    Second,
    Third,
}

fn main() {
    println!("{} {} {}", Level::High > Level::Low, Level::Low > Level::Below, Level::High > Level::Below);
    println!("{} {}", Byte::Two < Byte::Max, Byte::One < Byte::Two);
    let mut levels = vec![Level::High, Level::Below, Level::Low];
    levels.sort();
    println!("{levels:?} {:?} {:?}", levels.iter().max(), Level::Low.cmp(&Level::High));
    println!("{:?} {}", Level::High.partial_cmp(&Level::Below), Plain::Third > Plain::First);
    println!("{:?}", [Plain::Second, Plain::First].iter().min());
}
