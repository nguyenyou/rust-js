// The fib.js milestone: integers, `if`, loops, `match`, and simple enums.

/// Recursive, with `if` as an expression.
pub fn fib(n: u32) -> u32 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

/// Recursive, with `match` on integers.
pub fn fib_match(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fib_match(n - 1) + fib_match(n - 2),
    }
}

/// Iterative, with `while` and mutable locals.
pub fn fib_iter(n: u32) -> u32 {
    let mut a = 0;
    let mut b = 1;
    let mut i = 0;
    while i < n {
        let next = a + b;
        a = b;
        b = next;
        i += 1;
    }
    a
}

/// `loop` with `break value`.
pub fn fib_loop(n: u32) -> u32 {
    let mut a = 0;
    let mut b = 1;
    let mut i = 0;
    loop {
        if i == n {
            break a;
        }
        let next = a + b;
        a = b;
        b = next;
        i += 1;
    }
}

pub enum Order {
    Ascending,
    Descending,
}

/// Simple enum + `match`.
pub fn nth(order: Order, n: u32) -> u32 {
    match order {
        Order::Ascending => fib_iter(n),
        Order::Descending => fib_iter(20 - n),
    }
}

/// Signed wrapping arithmetic.
pub fn wrap_demo(x: i32) -> i32 {
    x * 3 - 7
}

/// Integer division panics on zero (and on `i32::MIN / -1`), just like Rust.
pub fn ratio(a: i32, b: i32) -> i32 {
    a / b
}
