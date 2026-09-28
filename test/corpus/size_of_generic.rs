//@ compile-fail: rust-js does not support `size_of` of a type parameter yet
// rust-js compiles a generic function once, for every type it's called
// with, so `size_of::<T>()` has no one size to be.
use std::mem::size_of;

fn size<T>() -> usize {
    size_of::<T>()
}

fn main() {
    println!("{} {}", size::<u8>(), size::<u32>());
}
