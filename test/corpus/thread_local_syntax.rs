// `thread_local!`'s forms: a block of declarations, with attributes and
// visibility, and one whose last declaration has no `;`.
use std::cell::{Cell, RefCell};

thread_local! {
    /// A counter.
    static COUNT: Cell<u32> = Cell::new(0);
    pub(crate) static NAMES: RefCell<Vec<String>> = RefCell::new(Vec::new());
}
thread_local!(static LIMIT: u32 = 3);

fn bump() -> u32 {
    COUNT.with(|c| {
        c.set(c.get() + 1);
        c.get()
    })
}

fn main() {
    for _ in 0..LIMIT.with(|l| *l) {
        bump();
    }
    NAMES.with(|n| n.borrow_mut().push("a".to_string()));
    println!("{} {:?}", COUNT.with(|c| c.get()), NAMES.with(|n| n.borrow().clone()));
}
