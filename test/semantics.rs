//! Differential regressions for effects hidden by readable JS shortcuts.
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

fn mark(log: &Cell<i32>, digit: i32) -> i32 {
    log.set(log.get() * 10 + digit);
    digit
}

pub fn entry_eager(present: bool) -> Vec<i32> {
    let log = Cell::new(0);
    let mut map = HashMap::new();
    if present { map.insert(1, 10); }
    *map.entry(mark(&log, 1)).or_insert(mark(&log, 2)) += mark(&log, 3);
    vec![log.get(), map[&1]]
}

pub fn entry_overwrite(present: bool) -> Vec<i32> {
    let log = Cell::new(0);
    let mut map = HashMap::new();
    if present { map.insert(1, 10); }
    *map.entry(mark(&log, 1)).or_insert(mark(&log, 2)) = mark(&log, 3);
    vec![log.get(), map[&1]]
}

pub fn entry_lazy(present: bool) -> Vec<i32> {
    let log = Cell::new(0);
    let mut map = HashMap::new();
    if present { map.insert(1, 10); }
    *map.entry(mark(&log, 1)).or_insert_with(|| mark(&log, 2)) = mark(&log, 3);
    vec![log.get(), map[&1]]
}

pub fn checked_overwrite(present: bool) -> Vec<i32> {
    let log = Cell::new(0);
    let mut map = HashMap::new();
    if present { map.insert(1, 10); }
    *map.get_mut(&mark(&log, 1)).unwrap() = mark(&log, 3);
    vec![log.get(), map[&1]]
}

struct Counted { count: Rc<Cell<i32>>, value: i32 }
impl Clone for Counted {
    fn clone(&self) -> Self {
        self.count.set(self.count.get() + 1);
        Counted { count: self.count.clone(), value: self.value + 1 }
    }
}

pub fn clones(n: usize) -> Vec<i32> {
    let count = Rc::new(Cell::new(0));
    let values = vec![Counted { count: count.clone(), value: 1 }; n];
    let mut result = vec![count.get()];
    for value in values { result.push(value.value); }
    result
}

struct Incremented { value: i32 }
impl Clone for Incremented {
    fn clone(&self) -> Self { Incremented { value: self.value + 1 } }
}
#[derive(Clone)]
struct Wrapped { value: Incremented }

pub fn rebuilt_clones(n: usize) -> Vec<i32> {
    let values = vec![Wrapped { value: Incremented { value: 1 } }; n];
    values.iter().map(|x| x.value.value).collect()
}

pub fn clone_evaluation(n: usize) -> Vec<i32> {
    let log = Cell::new(0);
    let values = vec![Incremented { value: mark(&log, 1) }; { mark(&log, 2); n }];
    let mut result = vec![log.get()];
    for value in values { result.push(value.value); }
    result
}

pub fn entry_trait_assignment(present: bool) -> Vec<i32> {
    let log = Cell::new(0);
    let mut map = HashMap::new();
    if present { map.insert(1, 10); }
    *map.entry(mark(&log, 1)).or_insert(mark(&log, 2)) += &mark(&log, 3);
    vec![log.get(), map[&1]]
}

thread_local! { static DEFAULTS: Cell<i32> = Cell::new(0); }
struct DefaultValue { n: i32 }
impl Default for DefaultValue {
    fn default() -> Self {
        DEFAULTS.with(|c| c.set(c.get() + 1));
        DefaultValue { n: 7 }
    }
}
pub fn entry_default(present: bool) -> Vec<i32> {
    DEFAULTS.with(|c| c.set(0));
    let mut map = HashMap::new();
    if present { map.insert(1, DefaultValue { n: 10 }); }
    let n = map.entry(1).or_default().n;
    vec![n, DEFAULTS.with(|c| c.get())]
}

fn repeat<T: Clone>(value: T, n: usize) -> Vec<T> { vec![value; n] }
pub fn generic_clones(n: usize) -> Vec<i32> {
    let values = repeat(Incremented { value: 1 }, n);
    values.iter().map(|x| x.value).collect()
}
