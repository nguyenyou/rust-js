// `Result` (ADR 0035): an enum like any other (ADR 0033), `{ TAG: "Ok", _0: v }`,
// with `?` returning an `Err` or a `None` as it is, and the usual methods.

pub fn parse_digit(c: char) -> Result<u32, String> {
    match c {
        '0' => Ok(0),
        '1' => Ok(1),
        '2' => Ok(2),
        '3' => Ok(3),
        _ => Err(c.to_string() + " isn't a digit I know"),
    }
}

/// `?` on a `Result`: the first `Err` is the answer.
pub fn sum_digits(a: char, b: char) -> Result<u32, String> {
    let x = parse_digit(a)?;
    let y = parse_digit(b)?;
    Ok(x * 10 + y)
}

/// `?` on an `Option`.
pub fn halves(n: u32) -> Option<u32> {
    let a = half(n)?;
    let b = half(a)?;
    Some(b)
}

fn half(n: u32) -> Option<u32> {
    if n % 2 == 0 { Some(n / 2) } else { None }
}

pub fn methods(c: char) -> (bool, bool, u32, Option<u32>) {
    let r = parse_digit(c);
    (r.is_ok(), r.is_err(), r.clone().unwrap_or(99), r.ok())
}

pub fn unwrapped(c: char) -> u32 {
    parse_digit(c).unwrap()
}

pub fn expected(c: char) -> u32 {
    parse_digit(c).expect("a digit")
}
