//@ compile-fail: constants of type `std::iter::Empty<u32>`
// A constant is its value, worked out by rustc; a std struct's value is its
// private fields, which aren't the JS value rust-js makes of it.
const NOTHING: std::iter::Empty<u32> = std::iter::empty();

fn main() {
    for i in NOTHING {
        println!("{i}");
    }
}
