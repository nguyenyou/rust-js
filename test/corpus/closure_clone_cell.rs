//@ compile-fail: cloning a closure that changes what it captured
// A `Cell` captured by value is copied by the clone, natively.
use std::cell::Cell;

fn main() {
    let count = Cell::new(0);
    let counter = move || {
        count.set(count.get() + 1);
        count.get()
    };
    let counter2 = counter.clone();
    println!("{} {}", counter(), counter2());
}
