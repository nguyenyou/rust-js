// Common std methods (ADR 0136): an `Option`'s `take` and `replace`,
// `mem::take`, `cmp::min` and `max`, ASCII case, `inspect`, a `Result`'s
// `as_ref`, `into_inner`, `append`, and `Debug`'s builders.

use std::cell::{Cell, RefCell};
use std::cmp::{max, min};
use std::collections::{BTreeMap, VecDeque};
use std::fmt;

struct Node {
    value: i32,
    next: Option<Box<Node>>,
}

struct Stack {
    head: Option<Box<Node>>,
}

impl Stack {
    fn push(&mut self, value: i32) {
        let next = self.head.take();
        self.head = Some(Box::new(Node { value, next }));
    }

    fn pop(&mut self) -> Option<i32> {
        self.head.take().map(|node| {
            self.head = node.next;
            node.value
        })
    }
}

fn swap_in<T>(slot: &mut Option<T>, value: T) -> Option<T> {
    slot.replace(value)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u8, &'static str);

struct Point {
    x: i32,
    label: String,
    tags: Vec<u8>,
}

impl fmt::Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Point")
            .field("x", &self.x)
            .field("label", &self.label)
            .field("tags", &self.tags)
            .finish()
    }
}

struct Secret(u8);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Secret").field("hint", &self.0).finish_non_exhaustive()
    }
}

struct Pair(i32, &'static str);

impl fmt::Debug for Pair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Pair").field(&self.0).field(&self.1).finish()
    }
}

struct Bag(Vec<i32>);

impl fmt::Debug for Bag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entry(&0).entries(self.0.iter()).finish()
    }
}

struct Letters(Vec<char>);

impl fmt::Debug for Letters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_set().entries(self.0.iter()).finish()
    }
}

struct Table(BTreeMap<String, u32>);

impl fmt::Debug for Table {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_map().entries(self.0.iter()).finish()
    }
}

fn main() {
    let mut a = Some(3);
    let taken = a.take();
    let mut b = Some(1);
    let old = b.replace(5);
    let mut slot = None;
    let first = swap_in(&mut slot, "x".to_string());
    let second = swap_in(&mut slot, "y".to_string());
    println!("{:?} {:?} {:?} {:?} {:?} {:?} {:?}", taken, a, old, b, first, second, slot);
    let mut stack = Stack { head: None };
    stack.push(1);
    stack.push(2);
    println!("{:?} {:?} {:?}", stack.pop(), stack.pop(), stack.pop());
    let mut items = vec![1, 2];
    let mut text = String::from("hi");
    let (items_taken, text_taken) = (std::mem::take(&mut items), std::mem::take(&mut text));
    println!("{:?} {:?} {:?} {:?}", items_taken, items, text_taken, text);

    println!(
        "{} {} {} {:?} {:?}",
        min(3, 7),
        max(-2i64, 5),
        max("a", "b"),
        min(Version(1, "x"), Version(1, "w")),
        max(Version(2, "a"), Version(2, "a"))
    );
    println!(
        "{} {} {} {}",
        "HeLLo Ünï".to_ascii_lowercase(),
        "abc é".to_ascii_uppercase(),
        "Hello".eq_ignore_ascii_case("hELLO"),
        "é".eq_ignore_ascii_case("É")
    );

    let doubled: Vec<i32> = [1, 2, 3].iter().inspect(|x| print!("saw {} ", x)).map(|x| x * 2).collect();
    println!("{:?}", doubled);
    let r: Result<String, i32> = Ok("abc".into());
    println!("{:?} {:?}", r.as_ref().map(|s| s.to_uppercase()), r);
    let cell = RefCell::new(vec![1]);
    cell.borrow_mut().push(2);
    println!("{:?} {}", cell.into_inner(), Cell::new(5).into_inner());
    let mut front = vec![1];
    let mut back = vec![2, 3];
    front.append(&mut back);
    let mut queue = VecDeque::from(vec![9]);
    let mut more = VecDeque::from(vec![8, 7]);
    queue.append(&mut more);
    println!("{:?} {:?} {:?} {:?}", front, back, queue, more);
    println!("{} {} {} {}", 3.0f64.exp2(), 0.5f32.exp2(), 1e-10f64.exp_m1(), 1e-10f64.ln_1p());

    println!("{:?}", Point { x: 1, label: "a".into(), tags: vec![2, 3] });
    println!("{:?} {:?} {:?} {:?}", Secret(4), Pair(5, "six"), Bag(vec![1, 2]), Bag(vec![]));
    let mut table = BTreeMap::new();
    table.insert("k".to_string(), 7);
    println!("{:?} {:?} {:?}", Letters(vec!['a', 'b']), Table(table), Table(BTreeMap::new()));
}
