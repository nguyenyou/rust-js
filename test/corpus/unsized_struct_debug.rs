//@ compile-fail: whose last field is a `dyn`
// A `dyn Debug` is the string it shows (ADR 0060), which a struct's last
// field made into one isn't: rejected.
use std::fmt::Debug;

struct Node<T: ?Sized> {
    size: usize,
    tail: T,
}

fn main() {
    let node: Box<Node<dyn Debug>> = Box::new(Node { size: 1, tail: 5 });
    println!("{} {:?}", node.size, &node.tail);
}
