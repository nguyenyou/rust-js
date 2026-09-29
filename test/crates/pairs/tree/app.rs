//! A clone of it is a whole new tree: growing one leaves the other.
pub fn main() {
    let mut tree = dep::sample();
    let kept = tree.clone();
    tree.grow();
    println!("{:?}", tree);
    println!("{:?} {}", kept, kept == dep::sample());
}
