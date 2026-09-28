//@ edition: 2015
//@ compile-fail: rust-js does not support a panic whose payload isn't text yet
// A panic of another value, `panic!(5)`, has no message in Rust, only a
// payload, and JS has none to give.
fn main() {
    let few = vec![1, 2];
    if few.len() > 100 {
        panic!(5);
    }
    println!("fine");
}
