// `Option`: `Some(x)` is `x` itself and `None` is `undefined`, as in
// ReScript (ADR 0030). A JS `null` counts as `None` too.

use std::cell::Cell;

pub fn half(n: i32) -> Option<i32> {
    if n % 2 == 0 { Some(n / 2) } else { None }
}

/// `match`, with patterns inside `Some`, and a guard.
pub fn describe(o: Option<i32>) -> i32 {
    match o {
        Some(0) => 100,
        Some(n) if n < 0 => -1,
        Some(n) => n * 2,
        None => 0,
    }
}

/// `if let`, and its `else`.
pub fn half_or_zero(n: i32) -> i32 {
    if let Some(h) = half(n) { h } else { 0 }
}

/// `while let`.
pub fn halvings(mut n: i32) -> u32 {
    let mut count = 0;
    while let Some(h) = half(n) {
        if h == 0 {
            break;
        }
        n = h;
        count += 1;
    }
    count
}

pub fn methods(n: i32) -> (bool, bool, i32) {
    let h = half(n);
    (h.is_some(), h.is_none(), h.unwrap_or(-1))
}

pub fn unwrapped(n: i32) -> i32 {
    half(n).unwrap()
}

pub fn expected(n: i32) -> i32 {
    half(n).expect("an even number")
}

/// `unwrap_or` evaluates its argument even when it isn't needed.
pub fn eager(n: i32) -> (i32, i32) {
    let calls = Cell::new(0);
    let bump = || {
        calls.set(calls.get() + 1);
        7
    };
    let v = half(n).unwrap_or(bump());
    (v, calls.get())
}

/// In a struct, and `==`, which counts `None` as equal to `None`.
#[derive(Clone, Copy, PartialEq)]
pub struct Slot {
    pub id: u32,
    pub value: Option<i32>,
}

pub fn fill(slot: Slot, n: i32) -> Slot {
    Slot { value: half(n), ..slot }
}

pub fn same(a: Option<i32>, b: Option<i32>) -> bool {
    a == b
}

pub fn same_slots(a: Slot, b: Slot) -> bool {
    a == b
}

/// A string inside.
pub fn label(n: i32) -> String {
    let name = if n > 0 { Some(n.to_string()) } else { None };
    match name {
        Some(s) => s + "!",
        None => String::from("none"),
    }
}

fn double(n: i32) -> i32 {
    n * 2
}

fn pair(n: i32) -> Option<(i32, i32)> {
    if n > 0 { Some((n, n + 1)) } else { None }
}

/// `map`: the closure runs on the value, for `Some` only.
pub fn mapped(n: i32) -> (Option<i32>, Option<i32>, Option<bool>, Option<i32>) {
    let h = half(n);
    (half(n).map(|h| h + 1), h.map(double), h.map(|x| x > 2), half(n).map(|h| h + 1).map(|x| x * 3))
}

/// `map` with a closure of statements, and with a pattern for its parameter.
pub fn mapped_more(n: i32) -> (Option<i32>, Option<i32>, u32, Option<i32>) {
    let calls = Cell::new(0);
    let counted = half(n).map(|h| {
        calls.set(calls.get() + 1);
        h - 1
    });
    (pair(n).map(|(a, b)| a * b), counted, calls.get(), half(n).map(|_| 7))
}
