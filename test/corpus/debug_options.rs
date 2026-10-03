// A `{:?}`'s options are given to each part of what it shows (ADR 0058):
// `{:5?}` of `Some(1)` pads the `1`, not the whole. A number, a `bool` and
// `()` apply them, as std's `fmt`s do, and a string's `Debug` doesn't.
use std::collections::BTreeMap;

fn main() {
    println!("[{:5?}] [{:<4?}] [{:+?}] [{:.1?}]", Some(1), vec![5, 6], Some(6), (1.25f64, 2.0f64));
    println!("[{:>3?}] [{:5?}] [{:06?}] [{:5?}]", (true, ()), Some("ab"), [-1i32, 2], true);
    println!("[{:#5?}]", Some(7));
    let mut m = BTreeMap::new();
    m.insert(1, 2.5);
    println!("[{:4?}] [{:x>4?}]", m, Ok::<u8, ()>(3));
}
