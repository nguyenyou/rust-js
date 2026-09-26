//! `async fn`, `.await`, `async` blocks and closures, and `spawn` (ADR 0029).

use std::cell::RefCell;
use std::rc::Rc;

use web::{Promise, response, spawn, window};

unsafe extern "Rust" {
    /// Resolves with `value` after `ms` milliseconds.
    #[link_name = "node:timers/promises#setTimeout"]
    safe fn later(ms: u32, value: u32) -> Promise<u32>;
}

async fn double(x: u32) -> u32 {
    later(1, x).await * 2
}

/// `.await` in the middle of an expression, in Rust's order.
pub async fn sum(a: u32, b: u32) -> u32 {
    double(a).await + double(b).await
}

/// A parameter that changes, and one taken apart.
pub async fn countdown(mut n: u32) -> u32 {
    let mut steps = 0;
    while n > 0 {
        n = later(0, n - 1).await;
        steps += 1;
    }
    steps
}

pub async fn swap((a, b): (u32, u32)) -> (u32, u32) {
    (later(0, b).await, a)
}

/// An `async` block, and an `async` closure.
pub async fn blocks(x: u32) -> u32 {
    let block = async move { double(x).await + 1 };
    let add = async |y: u32| later(0, y).await + x;
    block.await + add(10).await
}

/// A future held in a variable, awaited later.
pub async fn held() -> u32 {
    let first = later(5, 1);
    let second = double(2);
    first.await + second.await
}

/// A spawned task runs up to its first `.await` right away (a JS promise is
/// already running), and the rest later.
pub fn spawned() -> Rc<RefCell<Vec<u32>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let task_log = log.clone();
    spawn(Box::new(async move {
        task_log.borrow_mut().push(1);
        later(0, 0).await;
        task_log.borrow_mut().push(3);
    }));
    log.borrow_mut().push(2);
    log
}

/// `fetch`, from the web crate: its promises, awaited one after the other.
pub async fn load(url: &str) -> (u16, bool, String) {
    let response = window::fetch_with_str(window, url).await;
    let body = response::text(response).await;
    (response::status(response), response::ok(response), body)
}
