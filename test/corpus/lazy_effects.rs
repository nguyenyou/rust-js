// `a || b` and `a && b` run `b` only if `a` doesn't decide: when `b` is a
// call that needs statements in JS, as one given `&mut y` of a number does,
// they run only then too, in a `let`, an argument, an `if` and a `while`.
// From rustc's lazy-and-or.rs, where rust-js ran the call anyway.
fn bump(x: &mut i32) -> bool {
    *x += 1;
    println!("bump called");
    false
}

fn main() {
    let (t, f) = (true, false);
    let mut y = 10;
    let a = t || bump(&mut y);
    println!("{a} {y}");
    println!("{}", t || bump(&mut y));
    if t || bump(&mut y) {
        println!("if ||");
    }
    if f && bump(&mut y) {
        println!("never");
    }
    let b = f || bump(&mut y);
    println!("{b} {y}");
    let mut n = 0;
    while n < 2 && (f || !bump(&mut y)) {
        n += 1;
    }
    println!("{y} {n}");
}
