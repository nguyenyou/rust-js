//@ run-fail: end byte index 20 is out of bounds for string of length 6
// Slicing past the end panics, counting the string's bytes, as Rust does.
fn main() {
    let s = "héllo";
    println!("{}", &s[3..]);
    println!("{}", &s[3..20]);
}
