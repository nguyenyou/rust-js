//@ run-fail: assertion `left == right` failed: totals differ\n  left: 6\n right: 7
// `assert_eq!`'s message, in full, as Rust writes it.
fn main() {
    let total: i32 = [1, 2, 3].iter().sum();
    assert_eq!(total, 6);
    println!("first holds");
    assert_eq!(total, 7, "totals differ");
}
