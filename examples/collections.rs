// Vec, for loops, RefCell and `&mut` to objects (ADR 0025).

use std::cell::RefCell;
use std::rc::Rc;

pub struct Todo {
    pub id: u32,
    pub done: bool,
}

/// A range, counted with `for (let i = ..; i < ..; i++)`.
pub fn sum_to(n: u32) -> u32 {
    let mut total = 0;
    for i in 0..n {
        total += i;
    }
    total
}

/// Rust works out a range's end once: growing `n` in the loop doesn't
/// make it run longer.
pub fn end_once(n: i32) -> i32 {
    let mut n = n;
    let mut times = 0;
    for _ in 0..n {
        n += 1;
        times += 1;
    }
    times * 1000 + n
}

/// A `mut` loop variable is a copy: changing it doesn't move the loop on.
pub fn mut_counter(n: i32) -> i32 {
    let mut total = 0;
    for mut i in 0..n {
        i *= 10;
        total += i;
    }
    total
}

/// A `Vec` is a JS array.
pub fn evens(n: u32) -> Vec<u32> {
    let mut v = Vec::new();
    for i in 0..n {
        if i % 2 == 0 {
            v.push(i);
        }
    }
    v
}

pub fn keep_over(limit: i32) -> Vec<i32> {
    let mut v = vec![5, 1, 8, 3, 9, 2];
    v.retain(|x| *x > limit);
    v
}

pub fn lengths(n: u32) -> (usize, bool) {
    let mut v = evens(n);
    let before = v.len();
    v.clear();
    (before, v.is_empty())
}

/// Iterating by reference, by `iter()`, and over an array of tuples.
pub fn iterate(n: u32) -> u32 {
    let v = evens(n);
    let mut total = 0;
    for x in &v {
        total += *x;
    }
    for x in v.iter() {
        total += *x * 100;
    }
    for (k, w) in [(1, 2), (3, 4)] {
        total += k * w;
    }
    total
}

/// `break` and `continue`, including to an outer loop.
pub fn labeled(n: u32) -> u32 {
    let mut found = 0;
    'outer: for i in 0..n {
        for j in 0..n {
            if j > i {
                continue 'outer;
            }
            if i * j > 20 {
                break 'outer;
            }
            found += 1;
        }
    }
    found
}

/// `&mut` to a struct is the object: changes through it are the caller's.
fn toggle(todos: &mut Vec<Todo>, id: u32) {
    for todo in todos {
        if todo.id == id {
            todo.done = !todo.done;
        }
    }
}

pub fn toggled(n: u32) -> (u32, u32) {
    let mut todos = Vec::new();
    for id in 0..n {
        todos.push(Todo { id, done: id % 3 == 0 });
    }
    toggle(&mut todos, 1);
    toggle(&mut todos, 3);
    for todo in &mut todos {
        todo.id += 100;
    }
    todos.retain(|t| !t.done);
    let mut ids = 0;
    let mut done = 0;
    for todo in &todos {
        ids += todo.id;
        if todo.done {
            done += 1;
        }
    }
    (ids, done)
}

/// A `RefCell` is `{ value }`: `*c.borrow_mut() += 1` changes its value.
pub fn cell(times: i32) -> i32 {
    let c = RefCell::new(0);
    for _ in 0..times {
        *c.borrow_mut() += 2;
    }
    *c.borrow() + 1
}

/// Shared through an `Rc`, a `RefCell<Vec<_>>` is one array everyone changes.
pub fn shared(times: i32) -> (i32, usize) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let writer = log.clone();
    for i in 0..times {
        writer.borrow_mut().push(i * i);
    }
    let v = log.borrow();
    let mut total = 0;
    for x in v.iter() {
        total += *x;
    }
    (total, v.len())
}

/// Strings: `trim`, `is_empty`, `==`, `String::from`.
pub fn words(s: &str) -> (String, bool, bool) {
    let owned = String::from(s);
    let trimmed = owned.trim();
    (trimmed.to_string(), trimmed.is_empty(), trimmed == "hi")
}
