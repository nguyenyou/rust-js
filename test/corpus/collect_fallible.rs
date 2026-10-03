// `collect()` into a `Result` or an `Option` is the first `Err` or `None`,
// or all the values: and it stops there, as Rust's does, so what comes
// after isn't worked out (ADR 0139).
use std::collections::VecDeque;
use std::num::ParseIntError;

fn parse_all(v: &[&str]) -> Result<Vec<u32>, ParseIntError> {
    v.iter().map(|s| s.parse::<u32>()).collect()
}

fn halves(v: &[u32]) -> Option<Vec<u32>> {
    v.iter().map(|&n| if n % 2 == 0 { Some(n / 2) } else { None }).collect()
}

fn main() {
    println!("{:?} {:?}", parse_all(&["1", "22"]), parse_all(&["1", "x", "3"]));
    println!("{:?} {:?}", halves(&[2, 4]), halves(&[2, 3, 4]));
    let mut seen = Vec::new();
    let r: Result<Vec<i32>, String> = [1, -2, 3]
        .iter()
        .map(|&n| {
            seen.push(n);
            if n < 0 { Err(format!("negative {n}")) } else { Ok(n * 10) }
        })
        .collect();
    println!("{:?} {:?}", r, seen);
    let nested: Option<Vec<Option<u8>>> = vec![Some(Some(1)), Some(None)].into_iter().collect();
    let none: Option<Vec<Option<u8>>> = vec![Some(Some(1)), None].into_iter().collect();
    println!("{:?} {:?}", nested, none);
    let deque: Result<VecDeque<u8>, ()> = vec![Ok(1), Ok(2)].into_iter().collect();
    println!("{:?}", deque);
}
