// `{:#?}`, pretty Debug (ADR 0137): each part on a line of its own, indented
// by four spaces, its own lines too, and ended by a comma, for std's types,
// derived and hand-written `Debug`s, and generic code; `f.alternate()` says
// which a `fmt` is asked for.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;

#[allow(dead_code)]
#[derive(Debug)]
struct Point {
    x: i32,
    label: String,
}

#[allow(dead_code)]
#[derive(Debug)]
struct Pair(u8, Option<Point>);

#[allow(dead_code)]
#[derive(Debug)]
enum Shape {
    Dot,
    Circle { r: f64 },
    Line(Point, Point),
}

#[derive(Debug)]
struct Unit;

#[allow(dead_code)]
#[derive(Debug)]
struct Wide {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
}

#[allow(dead_code)]
#[derive(Debug)]
struct Wrap<T> {
    inner: T,
    list: Vec<T>,
}

struct Built {
    x: i32,
    tags: Vec<u8>,
}

impl fmt::Debug for Built {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Built").field("x", &self.x).field("tags", &self.tags).finish_non_exhaustive()
    }
}

struct Listy(Vec<i32>);

impl fmt::Debug for Listy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entry(&0).entries(self.0.iter()).finish()
    }
}

// What `write!` writes is the same either way.
struct Raw(u8);

impl fmt::Debug for Raw {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[{} {:?}]", self.0, vec![self.0])
    }
}

struct Asks;

impl fmt::Debug for Asks {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() { write!(f, "pretty") } else { write!(f, "plain") }
    }
}

struct Shown(u8);

impl fmt::Display for Shown {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() { write!(f, "#{}", self.0) } else { write!(f, "{}", self.0) }
    }
}

struct Delegate(Vec<u8>);

impl fmt::Debug for Delegate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

fn pretty<T: fmt::Debug>(value: T) -> String {
    format!("{:#?}", value)
}

fn main() {
    println!("{:#?} {:#?}", vec![1, 2], Vec::<i32>::new());
    println!("{:#?}", Some(vec![("a", 1.5)]));
    println!("{:#?} {:#?}", (1, "x", None::<u8>), (7,));
    let mut map = BTreeMap::new();
    map.insert("k", vec![Some(1)]);
    let failed: Result<u8, String> = Err("bad".into());
    println!("{:#?} {:#?} {:#?}", map, RefCell::new([1u8]), failed);
    println!("{:#?} {:#?} {:?}", 1..4, "text", vec![1]);

    let point = Point { x: 1, label: "a".into() };
    println!("{:#?}\n{:?}", point, point);
    println!("{:#?}", Pair(2, Some(Point { x: 3, label: "b".into() })));
    println!(
        "{:#?}",
        vec![
            Shape::Dot,
            Shape::Circle { r: 1.5 },
            Shape::Line(Point { x: 0, label: "s".into() }, Point { x: 9, label: "e".into() })
        ]
    );
    println!("{:#?} {:#?}", Unit, Wide { a: 1, b: 2, c: 3, d: 4, e: 5, f: 6 });
    println!("{:#?}", Wrap { inner: 1u8, list: vec![2, 3] });

    println!("{:#?} {:#?} {:#?}", Built { x: 1, tags: vec![2] }, Listy(vec![5]), Listy(vec![]));
    println!("{:#?} {:?} {:#?} {:?} {:#} {}", Raw(4), Raw(4), Asks, Asks, Shown(7), Shown(7));
    println!("{:#?}", Delegate(vec![9]));
    println!("{}\n{}", pretty(Some((1, "a"))), pretty(Wrap { inner: Raw(1), list: vec![] }));
    // What `dbg!` writes: a `&dyn Debug` made in its arguments.
    let kept = Pair(1, None::<Point>);
    eprintln!("{:#?}", &kept as &dyn fmt::Debug);
    println!("{:?}", kept);
}
