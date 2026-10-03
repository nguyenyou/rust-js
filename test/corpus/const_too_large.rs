// rustc won't build a value tree past 100,000 nodes, so the constant is its
// initializer, lowered as code (ADR 0096).
const DATA: [u8; 200_000] = [42; 200_000];

fn main() {
    println!("{}", DATA[199_999]);
}
