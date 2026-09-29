//! The impl's dictionary, through a generic function of this crate's.
fn through<V: dep::Value>(v: &V) -> u32 {
    dep::get(v)
}

pub fn main() {
    println!("{} {}", through(&dep::Wrap("x".to_string())), through(&dep::Wrap(1u8)));
}
