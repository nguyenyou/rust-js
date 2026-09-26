// Vec, for loops, RefCell and `&mut` to objects (ADR 0025).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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

/// Indexing (ADR 0056): `v[i]` of a `Vec` is a slice's, `$index(v, i)`,
/// which panics as Rust does past the end. A write checks first too,
/// `v[$at(v, i)] = x`, since JS would make the array longer.
pub fn indexed(i: usize) -> (u32, u32, Vec<u32>) {
    let mut v = vec![10, 20, 30];
    let read = v[i];
    v[i] = read + 1;
    v[0] += 5;
    let slice: &mut [u32] = &mut v;
    slice[2] *= 2;
    (read, v[i], v)
}

#[derive(Clone, Copy)]
pub struct Cell2 {
    pub hits: u32,
}

/// An element's fields are written in place, and a copy of one is its own.
pub fn element_fields(i: usize) -> (u32, u32, u32) {
    let mut cells = vec![Cell2 { hits: 0 }, Cell2 { hits: 0 }];
    cells[i].hits += 1;
    let before = cells[i];
    let r = &mut cells[i];
    r.hits = 10;
    (before.hits, cells[i].hits, cells[1 - i].hits)
}

/// An array changed in place is copied when it's copied.
pub fn arrays(i: usize) -> (u32, u32) {
    let mut a = [1, 2, 3];
    let b = a;
    a[i] = 9;
    (a[i], b[i])
}

/// `HashMap` and `HashSet` (ADR 0059): a JS `Map` and `Set`, keyed by what
/// JS compares by value. Their order isn't Rust's, which is arbitrary too,
/// so these sort what they take out.
pub fn word_counts(text: &str) -> Vec<(String, u32)> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for word in text.split(' ') {
        *counts.entry(word.to_string()).or_insert(0) += 1;
    }
    let mut all: Vec<(String, u32)> = counts.into_iter().collect();
    all.sort();
    all
}

pub fn map_basics(n: u32) -> (Option<u32>, Option<u32>, bool, usize, Option<u32>, u32) {
    let mut m = HashMap::new();
    m.insert("a", n);
    let old = m.insert("a", n + 1);
    m.insert("b", 3);
    let removed = m.remove("b");
    let first = m.get("a").copied();
    let mut total = 0;
    for (_, v) in &m {
        total += v;
    }
    *m.get_mut("a").unwrap() += 10;
    (old, first, m.contains_key("b"), m.len(), removed, total + m["a"])
}

pub fn set_basics(n: u32) -> (bool, bool, bool, usize, Vec<u32>, String) {
    let mut s: HashSet<u32> = [3, 1, 2].into_iter().collect();
    let new = s.insert(n);
    let again = s.insert(n);
    let gone = s.remove(&1);
    let mut items: Vec<u32> = s.iter().copied().collect();
    items.sort();
    let one: HashMap<&str, u32> = HashMap::from([("k", n)]);
    (new, again, gone, s.len(), items, format!("{one:?}"))
}

/// `or_default()` of a `Vec` value, pushed to, and a clone of the map that
/// keeps its own `Vec`s.
pub fn grouped(n: u32) -> Vec<(u32, Vec<u32>)> {
    let mut groups: HashMap<u32, Vec<u32>> = HashMap::new();
    for i in 1..n {
        groups.entry(i % 3).or_default().push(i);
    }
    let copy = groups.clone();
    if let Some(zero) = groups.get_mut(&0) {
        zero.push(99);
    }
    let mut all: Vec<(u32, Vec<u32>)> = copy.into_iter().collect();
    all.sort();
    all
}

/// `BTreeMap` and `BTreeSet`: the same `Map` and `Set`, gone over in their
/// keys' order, which for an enum is the order its variants are declared in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Gold,
    Bronze,
    Silver,
}

pub fn sorted_maps(text: &str) -> (Vec<(String, u32)>, Vec<String>, String, String) {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for word in text.split(' ') {
        *counts.entry(word.to_string()).or_insert(0) += 1;
    }
    let mut order = Vec::new();
    for (word, n) in &counts {
        order.push(format!("{word}={n}"));
    }
    let set: BTreeSet<u32> = [30, 4, 100, 7].into_iter().collect();
    let tiers: BTreeMap<Tier, u32> = [(Tier::Silver, 2), (Tier::Gold, 3), (Tier::Bronze, 1)].into_iter().collect();
    (
        counts.into_iter().collect(),
        order,
        format!("{set:?} {:?}", set.iter().rev().collect::<Vec<_>>()),
        format!("{tiers:?} {}", tiers[&Tier::Bronze]),
    )
}
