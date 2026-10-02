//@ run-fail: start byte index 2 is not a char boundary; it is inside 'é' (bytes 1..3 of string)
// Slicing inside a character panics, as Rust does, naming it.
fn main() {
    let s = String::from("héllo");
    println!("{}", &s[0..1]);
    println!("{}", &s[2..4]);
}
