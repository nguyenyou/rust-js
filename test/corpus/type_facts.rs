// `size_of`, `align_of` and `type_name` of a type parameter are what the
// caller's type has: a generic function that asks is given each fact it
// asks, after its dictionaries, and passes its own on to what it calls.
use std::any::type_name;
use std::mem::{align_of, size_of};

struct Pair(u8, u32);

fn size<T>() -> usize {
    size_of::<T>()
}

fn describe<T>(_: &T) -> String {
    format!("{} {} {}", type_name::<T>(), size_of::<T>(), align_of::<T>())
}

fn relay<U: Clone>(u: U) -> String {
    describe(&u.clone())
}

fn later<T>() -> impl Fn() -> usize {
    || size_of::<T>()
}

fn both<A, B>() -> usize {
    size::<A>() + size::<B>()
}

fn main() {
    println!("{} {} {}", size::<u8>(), size::<Pair>(), size::<[u16; 3]>());
    println!("{}", describe(&1u64));
    println!("{}", describe(&Some(3u8)));
    println!("{}", relay(7u16));
    println!("{} {}", later::<u64>()(), both::<u32, (u8, u16)>());
}
