//@ run-fail: index out of bounds: the len is 3 but the index is 5
fn main() {
    let v = vec![1, 2, 3];
    let i = v.len() + 2;
    println!("reading {i}");
    println!("{}", v[i]);
}
