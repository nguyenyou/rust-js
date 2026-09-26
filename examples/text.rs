// `char`'s questions, `str`'s `parse`, `split_whitespace` and `lines`,
// casts between `char` and numbers, and slicing by a range (ADR 0063). A
// `char` is a one-character string, so `c.is_alphabetic()` is a regular
// expression of the Unicode property Rust uses.

pub fn chars(c: char) -> (bool, bool, bool, bool, bool, bool, bool, bool) {
    (
        c.is_whitespace(),
        c.is_alphabetic(),
        c.is_numeric(),
        c.is_alphanumeric(),
        c.is_uppercase(),
        c.is_lowercase(),
        c.is_control(),
        c.is_ascii(),
    )
}
pub fn ascii(c: char) -> (bool, bool, bool, bool, char, char, Option<u32>, bool) {
    (
        c.is_ascii_digit(),
        c.is_ascii_hexdigit(),
        c.is_ascii_punctuation(),
        c.is_ascii_whitespace(),
        c.to_ascii_uppercase(),
        c.to_ascii_lowercase(),
        c.to_digit(16),
        c.is_digit(8),
    )
}
pub fn casts(c: char) -> (u32, u8, char, char, u32) {
    let code = c as u32;
    let x: u32 = 7u8.into();
    (code, c as u8, 65u8 as char, (b'a' + (code % 26) as u8) as char, x)
}
pub fn parses(s: &str) -> String {
    let n = s.parse::<u32>().map_err(|e| e.to_string());
    let i: Result<i8, _> = s.parse();
    let f = s.parse::<f64>().map_err(|e| e.to_string());
    let b = s.parse::<bool>().map_err(|e| e.to_string());
    let c = s.parse::<char>().map_err(|e| e.to_string());
    let owned: String = s.parse().unwrap();
    format!("{n:?} {:?} {f:?} {b:?} {c:?} {owned:?}", i.map_err(|e| e.to_string()))
}
// `?` on a `parse`: its error converts to the function's.
fn sum(text: &str) -> Result<i32, std::num::ParseIntError> {
    let mut total = 0;
    for word in text.split_whitespace() {
        total += word.parse::<i32>()?;
    }
    Ok(total)
}
pub fn words(text: &str) -> (Vec<String>, Vec<String>, String, Vec<String>, bool) {
    let words = text.split_whitespace().map(|w| w.to_string()).collect();
    let lines = text.lines().map(|l| l.to_string()).collect();
    let total = match sum(text) {
        Ok(n) => format!("sum {n}"),
        Err(e) => format!("error: {e}"),
    };
    // A closure as the pattern.
    let pieces = text
        .split(|c: char| c.is_ascii_digit() || c == '\r')
        .map(|p| p.to_string())
        .collect();
    (words, lines, total, pieces, text.contains(|c: char| c.is_uppercase()))
}
pub fn slices(v: &[u32]) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, u32) {
    let a = [10u32, 20, 30];
    let tail: u32 = a[1..].iter().sum();
    (v[1..3].to_vec(), v[2..].to_vec(), v[..1].to_vec(), v[..].to_vec(), tail)
}
pub fn slice_panics(start: usize, end: usize) -> Vec<u32> {
    let v = vec![1u32, 2, 3];
    v[start..end].to_vec()
}
pub fn escapes() -> String {
    format!(
        "{:?} {:?} {:?} {:?} {:?}",
        "a\"b\\c\n\t\r\0\u{1b}\u{7f}é\u{3000}\u{a0}\u{200b}\u{2028}\u{e000}\u{301}x\u{10ffff}", 'a', '\'', '"', "it's"
    )
}
pub fn report() -> String {
    let mut out = String::new();
    for c in [
        'a', 'Z', '7', ' ', '\n', '\u{3000}', 'é', 'ß', '٣', 'Ⅻ', '!', '\u{feff}', '\u{7f}',
    ] {
        out.push_str(&format!("{c:?} {:?} {:?} {:?}\n", chars(c), ascii(c), casts(c)));
    }
    for s in [
        "42",
        "-1",
        "",
        "4x",
        "99999999999",
        "+7",
        "-128",
        "200",
        "1.5",
        "-inf",
        "NaN",
        "1e3",
        ".5",
        " 1",
    ] {
        out.push_str(&format!("{s:?} {}\n", parses(s)));
    }
    for s in ["true", "false", "True", "x", "é", "ab"] {
        out.push_str(&format!("{s:?} {}\n", parses(s)));
    }
    for text in [
        "1 2\t3",
        "a\u{3000}b  c\u{feff}d",
        "x\r\ny\n\nz\n",
        "  ",
        "4\n-5\r\n",
        "foo\nBar\n\r\nbaz\r",
        "",
        "\n",
        "\r",
    ] {
        out.push_str(&format!("{:?}\n", words(text)));
    }
    out.push_str(&format!("{:?}\n{}\n", slices(&[1, 2, 3, 4]), escapes()));
    out
}
