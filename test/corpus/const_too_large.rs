//@ compile-fail: rust-js does not support constants of type `[u8; 200000]` yet
// rustc won't build a value tree past 100,000 nodes, and reports that as its
// own error; rust-js reports the constant as unsupported instead.
const DATA: [u8; 200_000] = [42; 200_000];

fn main() {
    println!("{}", DATA[199_999]);
}
