//! A value of this crate's, with a destructor, in the library's `Wrap`.
#[derive(Clone)]
struct Loud;

impl Drop for Loud {
    fn drop(&mut self) {
        println!("dropped");
    }
}

pub fn main() {
    dep::eat(dep::Wrap(Loud));
    println!("end");
}
