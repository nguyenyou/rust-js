//@ run-fail: called `Option::unwrap()` on a `None` value
fn find(v: &[i32], x: i32) -> Option<usize> {
    v.iter().position(|&y| y == x)
}

fn main() {
    println!("{:?}", find(&[4, 5], 5));
    let at = find(&[4, 5], 6).unwrap();
    println!("{at}");
}
