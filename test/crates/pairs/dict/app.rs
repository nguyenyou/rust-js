//! A call of the generic function, which is given the impl's dictionary,
//! and the trait's methods called directly.
use dep::Value;

pub fn main() {
    println!("{} {} {}", dep::get(&dep::Seven), dep::Seven.value(), dep::Seven.twice());
}
