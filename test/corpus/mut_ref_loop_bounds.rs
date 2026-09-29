//@ run-fail: range end index 9 out of range for slice of length 4
// `for x in &mut v[a..b]` checks its range first, as `&mut v[a..b]` does
// (ADR 0099): out of bounds, it panics before the loop runs.
fn main() {
    let mut v = vec![1, 2, 3, 4];
    for x in &mut v[3..9] {
        *x = 0;
    }
    println!("{v:?}");
}
