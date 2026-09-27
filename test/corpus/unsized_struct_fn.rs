// A struct whose last field becomes a `dyn FnMut` holds the same JS
// function either way.
use std::cell::RefCell;

fn main() {
    let total = RefCell::new(0);
    let add: &RefCell<dyn FnMut(i32) -> i32> = &RefCell::new(|x| x * 2);
    for x in [1, 2, 3] {
        *total.borrow_mut() += (&mut *add.borrow_mut())(x);
    }
    println!("{}", total.borrow());
}
