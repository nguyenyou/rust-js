//@ edition: 2018
//@ run-fail: 3 left
// A `String` given to `panic!` before edition 2021 is its message too.
fn main() {
    let left = 3;
    let message = format!("{} left", left);
    if left > 0 {
        panic!(message);
    }
}
