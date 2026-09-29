//! A copy of it is its own: changing one through a `&mut` changes only it.
use dep::Shape;

pub fn main() {
    let mut s = dep::shape();
    let t = s;
    if let Shape::Circle(r) = &mut s {
        *r = 5;
    }
    println!("{:?} {:?} {:?} {:?}", s, t, dep::shape(), Shape::Dot);
}
