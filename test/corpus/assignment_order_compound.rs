//@ run-fail: index out of bounds: the len is 0 but the index is 0
// `v[i] += f()` runs `f` before `v[i]` is checked, as `v[i] = f()` does.
fn value(x: i32) -> i32 {
    println!("value");
    x
}

fn main() {
    let mut v: Vec<i32> = Vec::new();
    let i = 0;
    v[i] += value(1);
}
