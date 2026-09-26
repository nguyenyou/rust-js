//! `async fn`, `.await`, `async` blocks and closures, and `spawn` (ADR 0029),
//! and the web crate's promises: `fetch`, binary data and WebAssembly.

use std::cell::RefCell;
use std::rc::Rc;

use web::{JsObject, Promise, Uint8Array, array_buffer, response, spawn, uint8_array, web_assembly, web_assembly_instance, window};

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

/// Binary data: `bytes()`, `arrayBuffer()`, and a view of a buffer.
pub async fn load_bytes(url: &str) -> (u32, u32, u32) {
    let response = window::fetch_with_str(window, url).await;
    let copy = response::clone(response);
    let bytes = response::bytes(response).await;
    let buffer = response::array_buffer(copy).await;
    let view = uint8_array::new(buffer);
    (uint8_array::length(bytes), array_buffer::byte_length(buffer), uint8_array::length(view))
}

/// What a WebAssembly module imports: `{ env: { double } }`. The module
/// reads the fields; Rust never does.
#[allow(dead_code)]
struct Imports {
    env: Env,
}

#[allow(dead_code)]
struct Env {
    double: Box<dyn Fn(i32) -> i32>,
}

unsafe extern "Rust" {
    /// The module's export, called on its `exports` object.
    #[link_name = "add"]
    safe fn wasm_add(this: &JsObject, a: i32, b: i32) -> i32;
}

/// WebAssembly: compile a module, instantiate it with imports from Rust
/// (a struct, which is a JS object), and call its export, which calls back.
pub async fn run_wasm(bytes: &Uint8Array, a: i32, b: i32) -> i32 {
    let module = web_assembly::compile_with_uint8_array(bytes).await;
    let imports = Imports { env: Env { double: Box::new(|x| x * 2) } };
    let instance = web_assembly::instantiate_with_web_assembly_module_and_import_object(module, &imports).await;
    wasm_add(web_assembly_instance::exports(instance), a, b)
}

/// The other `instantiate`: from bytes, to a `{ module, instance }` dictionary.
pub async fn instantiate_bytes(bytes: &Uint8Array) -> bool {
    let imports = Imports { env: Env { double: Box::new(|x| x) } };
    let source = web_assembly::instantiate_with_import_object(uint8_array::buffer(bytes), &imports).await;
    web_assembly::validate_with_uint8_array(bytes) && wasm_add(web_assembly_instance::exports(source.instance), 1, 2) == 3
}
