// A byte string, `b"GET"`, is its bytes, `[71, 69, 84]`, as an array of
// `u8`s is (ADR 0126); and `s.as_bytes()` is a string's UTF-8 bytes, which
// JS's `TextEncoder` makes, a copy, as nothing can write through it.

fn method(line: &[u8]) -> &'static str {
    match line {
        b"GET" => "get",
        b"POST" => "post",
        [] => "empty",
        _ => "other",
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().map(|&b| b as u32).sum()
}

fn main() {
    let get = b"GET";
    println!("{:?} {} {}", get, get.len(), method(get));
    println!("{} {}", method(b"POST"), method(b""));
    // Escapes, and every byte, not just ASCII.
    let raw = b"a\x01\xff\n\\\"";
    println!("{:?} {}", raw, checksum(raw));
    let joined = [b"ab".as_slice(), b"cd"].concat();
    println!("{:?} {}", joined, joined == b"abcd");
    println!("{}", b"zz" > b"za");

    // A string's UTF-8 bytes: a multi-byte character is more than one.
    let word = "héllo €";
    let bytes = word.as_bytes();
    println!("{} {} {:?}", word.chars().count(), bytes.len(), &bytes[..3]);
    println!("{}", checksum("abc".as_bytes()));
    println!("{}", method("GET".as_bytes()));
}
