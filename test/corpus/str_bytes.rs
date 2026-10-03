// `s.bytes()` is an iterator of a string's UTF-8 bytes: the array
// `as_bytes()` is (ADR 0126), iterated. A multi-byte character is more than
// one, as in Rust.

// FNV-1a, as a hash of a string is often written.
fn fnv1a(text: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for b in text.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

// A number at the start of a line, read a byte at a time.
fn leading_number(line: &str) -> Option<u32> {
    let mut bytes = line.bytes().peekable();
    let mut value = None;
    while let Some(&b) = bytes.peek() {
        if !(b'0'..=b'9').contains(&b) {
            break;
        }
        value = Some(value.unwrap_or(0) * 10 + (b - b'0') as u32);
        bytes.next();
    }
    value
}

fn main() {
    let word = "héllo €";
    let all: Vec<u8> = word.bytes().collect();
    println!("{:?}", all);
    println!("{} {} {}", word.bytes().len(), word.bytes().count(), word.chars().count());
    println!("{}", word.bytes().filter(|&b| b >= 128).count());
    let sum: u32 = "abc".bytes().map(|b| b as u32).sum();
    println!("{sum}");
    println!("{:?}", "abc".bytes().rev().collect::<Vec<_>>());
    println!("{:?} {:?} {:?}", word.bytes().next(), word.bytes().last(), word.bytes().nth(2));
    println!("{:?}", word.bytes().position(|b| b == b' '));
    for (i, b) in "hi!".bytes().enumerate() {
        println!("{i}: {b}");
    }
    println!("{:08x} {:08x}", fnv1a(""), fnv1a("hello"));
    println!("{:?} {:?} {:?}", leading_number("42 apples"), leading_number("7"), leading_number("none"));
    // `len()` is what's left, and doesn't take it.
    let mut rest = "abc".bytes();
    rest.next();
    let before = rest.len();
    let next = rest.next();
    println!("{before} {next:?} {}", rest.len());
    println!("{}", [1, 2, 3].iter().skip(1).len());
}
