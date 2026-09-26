// State that lasts as long as the program (ADR 0037): `thread_local!`, as
// wasm-bindgen programs keep JS values. JS runs a module on one thread, so a
// thread-local is one of the module's variables: `const COUNT = { value: 0 };`.

use std::cell::{Cell, RefCell};

thread_local! {
    static COUNT: Cell<u32> = Cell::new(0);
    static LOG: RefCell<Vec<String>> = RefCell::new(Vec::new());
    static START: Cell<i32> = Cell::new(10 * 4 + 2);
}

/// Each call sees what the last one left.
pub fn bump() -> u32 {
    COUNT.set(COUNT.get() + 1);
    COUNT.get()
}

pub fn record(line: &str) -> u32 {
    LOG.with_borrow_mut(|log| log.push(line.to_string()));
    LOG.with_borrow(|log| log.len() as u32)
}

pub fn start() -> i32 {
    START.with(|s| s.get())
}
