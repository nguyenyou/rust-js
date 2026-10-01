// An inline `const { .. }` is its value, as rustc computes it (ADR 0127),
// and a match arm's `if let` guard binds what it matches for the arm, which
// is tried only where the pattern matches and the guard does.

const fn square(n: u32) -> u32 {
    n * n
}

fn generic_size<T>() -> usize {
    const { 4 }
}

fn classify(input: &str) -> String {
    match input.split_once('=') {
        Some((key, value)) if let Ok(n) = value.parse::<i32>() => format!("{key} is the number {n}"),
        Some((key, value))
            if let Some(rest) = value.strip_prefix('"')
                && let Some(inner) = rest.strip_suffix('"')
                && !inner.is_empty() =>
        {
            format!("{key} is the string {inner}")
        }
        Some((key, _)) => format!("{key} is something else"),
        None => "not a setting".to_string(),
    }
}

fn first_even(items: &[Option<i32>]) -> Option<i32> {
    for item in items {
        match item {
            Some(n) if let 0 = n % 2 => return Some(*n),
            _ => {}
        }
    }
    None
}

fn main() {
    let x = const { square(7) + 1 };
    let table = const { [square(1), square(2), square(3)] };
    let name = const { "fixed" };
    println!("{} {:?} {} {}", x, table, name, generic_size::<u8>());

    for input in ["a=1", "b=\"hi\"", "e=\"\"", "c=x", "d"] {
        println!("{}", classify(input));
    }
    println!("{:?} {:?}", first_even(&[Some(3), None, Some(8), Some(10)]), first_even(&[Some(1)]));
}
