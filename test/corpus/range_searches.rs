// An iterator's searches take it by `&mut`, so a range they're called on is
// borrowed: `(0..n).all(f)` searches the range, as `(0..n).map(f)` maps it.
fn main() {
    let n = 5;
    println!("{}", (0..n).all(|i| i < 5));
    println!("{}", (0..n).any(|i| i == 3));
    println!("{:?}", (1..=n).find(|i| i % 2 == 0));
    println!("{:?}", (0..n).position(|i| i * i > 5));
    println!("{}", (0u64..3).all(|i| i < 3));
}
