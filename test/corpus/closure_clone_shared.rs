// A clone of a closure that never changes what it holds can't be told from
// the closure itself, and one that shares a cell through an `Rc` shares it
// natively too.
use std::cell::Cell;
use std::rc::Rc;

fn main() {
    let base = 10;
    let add = move |x: i32| x + base;
    let add2 = add.clone();
    println!("{} {}", add(1), add2(2));

    let count = Rc::new(Cell::new(0));
    let counter = {
        let count = Rc::clone(&count);
        move || count.set(count.get() + 1)
    };
    let counter2 = counter.clone();
    counter();
    counter2();
    println!("{}", count.get());
}
