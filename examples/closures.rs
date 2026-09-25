// Closures: arrow functions, with Rust's capture rules (ADR 0022).

#[derive(Clone, Copy)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Captured by reference: the closure changes the variable itself.
pub fn by_reference(a: i32, b: i32) -> i32 {
    let mut total = 0;
    let mut add = |x: i32| total += x;
    add(a);
    add(b);
    total
}

/// `move` copies `n` into the closure, so changing `n` afterwards doesn't
/// change what the closure sees.
pub fn move_copies(n: i32) -> i32 {
    let mut n = n;
    let get = move || n;
    n += 100;
    get() * 1000 + n
}

/// A `move` closure owns its copy and can change it between calls.
pub fn own_state(start: i32) -> (i32, i32) {
    let mut count = start;
    let mut next = move || {
        count += 1;
        count
    };
    next();
    next();
    (next(), count)
}

/// Each time round the loop, a new closure takes a new copy of `n`.
pub fn fresh_copy_each_time(times: i32) -> i32 {
    let mut n = 0;
    let mut total = 0;
    let mut i = 0;
    while i < times {
        let mut bump = move || {
            n += 1;
            n
        };
        total += bump() * 10 + bump();
        i += 1;
    }
    total
}

/// A `Copy` struct, changed in place: the closure gets a real copy.
pub fn struct_copy(x: i32) -> (i32, i32) {
    let mut p = Point { x, y: 0 };
    let mut shift = move || {
        p.x += 1;
        p.x
    };
    shift();
    p.y = 5;
    (shift(), p.x + p.y)
}

/// Closures returned from functions, boxed.
fn adder(k: i32) -> Box<dyn Fn(i32) -> i32> {
    Box::new(move |x| x + k)
}

pub fn add_both(a: i32, b: i32) -> i32 {
    let add_a = adder(a);
    let add_b = adder(b);
    add_a(add_b(1))
}

/// A closure that takes a pattern, and one passed to another function.
fn apply(f: &dyn Fn((i32, i32)) -> i32, a: i32, b: i32) -> i32 {
    f((a, b))
}

pub fn pattern_param(a: i32, b: i32) -> i32 {
    let scale = 3;
    apply(&|(x, y)| x * scale - y, a, b)
}
