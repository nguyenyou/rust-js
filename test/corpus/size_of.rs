// `size_of`, `align_of` and a sized value's `size_of_val` are the wasm32
// target's, which rust-js checks programs for (ADR 0090), as their
// constants already are. Without a pointer or a `usize` in them, these types
// are the same size on the 64-bit machine the native side runs on.
use std::mem::{align_of, size_of, size_of_val};

#[allow(dead_code)]
#[repr(C)]
struct Pair {
    a: u8,
    b: u32,
}

#[allow(dead_code)]
struct Wide {
    a: i64,
    b: u16,
}

#[allow(dead_code)]
enum Small {
    A,
    B,
    C,
}

fn main() {
    println!("{} {} {} {}", size_of::<u8>(), size_of::<u32>(), size_of::<i64>(), size_of::<Pair>());
    println!("{} {} {}", align_of::<u32>(), align_of::<Pair>(), align_of::<Wide>());
    println!("{} {} {}", size_of::<[u16; 5]>(), size_of::<Small>(), size_of::<(u8, u32)>());
    let w = Wide { a: 1, b: 2 };
    println!("{} {}", size_of_val(&w), size_of::<Option<u8>>());
    let numbers = [1u32, 2, 3];
    println!("{} {}", size_of_val(&numbers), size_of_val(&w.b));
}
