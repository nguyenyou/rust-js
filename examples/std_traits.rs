// The crate's own impls of std traits (ADRs 0052, 0053): `Default`,
// `From`, `Clone` and `PartialEq`, hand-written and derived. A clone is a
// copy only where it could be told apart from the value: something changes
// one of them, or a hand-written `clone` makes something else. A
// hand-written `eq` decides wherever it is, and a `fmt` returns what it
// writes. An `Iterator` is a JS iterator, an `Ordering` is -1, 0 or 1, and
// `{:?}` shows a value by its type.

use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Default)]
pub struct Settings {
    pub size: u32,
    pub tags: Vec<String>,
    pub mode: Mode,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Mode {
    #[default]
    Off,
    On,
}

pub struct Config {
    pub retries: u32,
    pub name: String,
}

impl Default for Config {
    fn default() -> Self {
        Config { retries: 3, name: "main".to_string() }
    }
}

/// A clone is one generation later.
pub struct Tracked {
    pub generation: u32,
}

impl Clone for Tracked {
    fn clone(&self) -> Self {
        Tracked { generation: self.generation + 1 }
    }
}

#[derive(Clone)]
pub struct Holder {
    pub tracked: Tracked,
    pub label: String,
}

pub enum Figure {
    Dot,
    Poly(Vec<u32>),
}

impl Clone for Figure {
    fn clone(&self) -> Self {
        match self {
            Figure::Dot => Figure::Dot,
            Figure::Poly(points) => Figure::Poly(points.clone()),
        }
    }
}

#[derive(Clone)]
pub enum Shape {
    Dot,
    Poly(Vec<u32>),
}

pub struct Meters(pub f64);

impl From<f64> for Meters {
    fn from(m: f64) -> Self {
        Meters(m)
    }
}

impl From<u32> for Meters {
    fn from(km: u32) -> Self {
        Meters(km as f64 * 1000.0)
    }
}

pub fn fresh<T: Default>() -> T {
    T::default()
}

pub fn twice<T: Clone>(x: &T) -> Vec<T> {
    vec![x.clone(), x.clone()]
}

/// A `T: Copy` clones by copying.
pub fn copied<T: Copy>(x: &T) -> T {
    x.clone()
}

/// Derived and hand-written, direct and through a generic.
pub fn defaults() -> (u32, u32, bool, u32, String, usize) {
    let s = Settings::default();
    let c: Config = fresh();
    let n: u32 = fresh();
    (s.size, c.retries, s.mode == Mode::Off, n, c.name, s.tags.len())
}

/// A clone of a `Vec` that's changed is an array of its own.
pub fn vec_clones() -> (usize, usize) {
    let mut v = vec![1u32, 2];
    let w = v.clone();
    v.push(3);
    (v.len(), w.len())
}

/// A derived clone copies what changes: here `tags`, pushed to.
pub fn struct_clones() -> (usize, usize, u32, u32) {
    let s = Settings { size: 1, tags: vec!["a".to_string()], mode: Mode::On };
    let mut t = s.clone();
    t.tags.push("b".to_string());
    t.size = 2;
    (s.tags.len(), t.tags.len(), s.size, t.size)
}

/// A hand-written clone runs: called directly, from a derived one, from an
/// iterator's `cloned`, and through a generic.
pub fn hand_written() -> (u32, u32, u32, u32) {
    let a = Tracked { generation: 0 };
    let b = a.clone();
    let holder = Holder { tracked: b, label: "h".to_string() };
    let copy = holder.clone();
    let all = vec![Tracked { generation: 5 }];
    let cloned: Vec<Tracked> = all.iter().cloned().collect();
    let both = twice(&copy.tracked);
    (
        copy.tracked.generation,
        cloned.iter().map(|t| t.generation).sum(),
        both.iter().map(|t| t.generation).sum(),
        copied(&7u32),
    )
}

fn points(s: &Shape) -> usize {
    match s {
        Shape::Dot => 0,
        Shape::Poly(p) => p.len(),
    }
}

/// A derived clone of an enum copies the variant whose `Vec` changes.
pub fn enum_clones() -> (usize, usize, usize) {
    let a = Shape::Poly(vec![1]);
    let b = match a.clone() {
        Shape::Poly(mut p) => {
            p.push(2);
            Shape::Poly(p)
        }
        other => other,
    };
    let dot = Shape::Dot.clone();
    let f = Figure::Poly(vec![1, 2, 3]).clone();
    let n = match f {
        Figure::Dot => 0,
        Figure::Poly(p) => p.len(),
    };
    (points(&a), points(&b), points(&dot) + n)
}

pub fn conversions() -> (f64, f64, f64) {
    let a = Meters::from(2.5);
    let b: Meters = 3u32.into();
    let c: Meters = 1.5.into();
    (a.0, b.0, c.0)
}

/// Versions are equal when their majors are: the label doesn't count.
pub struct Version {
    pub major: u32,
    pub label: String,
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major
    }
}

impl Eq for Version {}

#[derive(PartialEq)]
pub struct Release {
    pub version: Version,
    pub notes: Vec<String>,
}

#[derive(PartialEq)]
pub enum Change {
    Nothing,
    Bump(Version),
    Note(String),
}

fn version(major: u32, label: &str) -> Version {
    Version { major, label: label.to_string() }
}

pub fn same<T: PartialEq>(a: &T, b: &T) -> bool {
    a == b
}

/// A `T: Eq` compares with `T`'s `PartialEq`.
pub fn count_equal<T: Eq>(items: &[T], x: &T) -> usize {
    items.iter().filter(|item| *item == x).count()
}

/// A hand-written `eq` decides, directly, from derived ones, and through generics.
pub fn equalities() -> (bool, bool, bool, bool, bool, bool) {
    let r1 = Release { version: version(1, "a"), notes: vec!["x".to_string()] };
    let r2 = Release { version: version(1, "b"), notes: vec!["x".to_string()] };
    let r3 = Release { version: version(1, "c"), notes: vec![] };
    (
        r1 == r2,
        r1 != r3,
        same(&version(2, "a"), &version(3, "a")),
        Change::Bump(version(4, "a")) == Change::Bump(version(4, "b")),
        Change::Note("n".to_string()) == Change::Nothing,
        Some(version(5, "a")) == Some(version(5, "b")),
    )
}

pub fn generic_equalities() -> (usize, bool, bool) {
    let all = vec![version(1, "a"), version(2, "b"), version(1, "c")];
    let pairs = vec![(1, "a"), (2, "b")];
    (
        count_equal(&all, &version(1, "z")),
        same(&pairs, &vec![(1, "a"), (2, "b")]),
        same(&vec![version(1, "a")], &vec![version(2, "a")]),
    )
}

/// A `PartialEq` with another type on the right: `!=` is its `eq` too.
impl PartialEq<f64> for Meters {
    fn eq(&self, other: &f64) -> bool {
        self.0 == *other
    }
}

pub fn compared() -> (bool, bool, bool) {
    let m = Meters::from(2.0);
    (m == 2.0, m != 3.0, m != 2.0)
}

/// `Display` (ADR 0054): a `fmt` returns the string it writes.
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

pub struct Route {
    pub stops: Vec<Point>,
    pub closed: bool,
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.stops.is_empty() {
            return f.write_str("nowhere");
        }
        for (i, stop) in self.stops.iter().enumerate() {
            if i > 0 {
                f.write_str(" -> ")?;
            }
            stop.fmt(f)?;
        }
        if self.closed {
            write_loop(f, self.stops.len())?;
        }
        Ok(())
    }
}

/// A helper that writes to the same formatter.
fn write_loop(f: &mut fmt::Formatter, stops: usize) -> fmt::Result {
    write!(f, " (a loop of {stops})")
}

impl fmt::Display for Figure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Figure::Dot => write!(f, "a dot"),
            Figure::Poly(points) => write!(f, "a polygon of {}", points.len()),
        }
    }
}

/// A generic impl: `T`'s `fmt` inside.
pub struct Labeled<T> {
    pub label: String,
    pub value: T,
}

impl<T: fmt::Display> fmt::Display for Labeled<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.label, self.value)
    }
}

pub fn shown<T: fmt::Display>(x: &T) -> String {
    format!("<{x}>")
}

pub fn displays() -> (String, String, String, String, String, String) {
    let p = Point { x: 1, y: -2 };
    let route = Route { stops: vec![Point { x: 0, y: 0 }, Point { x: 3, y: 4 }], closed: true };
    let empty = Route { stops: vec![], closed: false };
    let labeled = Labeled { label: "at".to_string(), value: Point { x: 5, y: 6 } };
    (
        p.to_string(),
        format!("{route} / {empty}"),
        format!("{} and {}", Figure::Dot, Figure::Poly(vec![1, 2])),
        labeled.to_string(),
        shown(&labeled),
        shown(&Labeled { label: "n".to_string(), value: 2.5 }),
    )
}

/// `Iterator` (ADR 0055): a JS iterator, whose adapters are lazy too.
pub struct Countdown {
    pub n: u32,
}

impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            Some(self.n + 1)
        }
    }
}

/// Endless: only lazy adapters can use it.
pub struct Fibonacci {
    pub a: u32,
    pub b: u32,
}

impl Iterator for Fibonacci {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        let a = self.a;
        self.a = self.b;
        self.b += a;
        Some(a)
    }
}

/// A generic one, whose `Some(())` and `Some(None)` must still be items.
pub struct Repeat<T> {
    pub item: T,
    pub times: u32,
}

impl<T: Clone> Iterator for Repeat<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.times == 0 {
            return None;
        }
        self.times -= 1;
        Some(self.item.clone())
    }
}

fn fibonacci() -> Fibonacci {
    Fibonacci { a: 0, b: 1 }
}

pub fn iterations() -> (u32, u32, Vec<u32>, u32, usize, Option<u32>) {
    let mut total = 0;
    for x in (Countdown { n: 3 }) {
        total += x;
    }
    let mut c = Countdown { n: 2 };
    let first = c.next().unwrap_or(0);
    (
        total,
        first,
        fibonacci().skip(1).take(6).collect(),
        fibonacci().take(5).map(|x| x * 2).sum(),
        Countdown { n: 4 }.count(),
        fibonacci().find(|&x| x > 50),
    )
}

pub fn generic_iterations() -> (usize, usize, Vec<u32>, Option<u32>) {
    (
        Repeat { item: (), times: 3 }.count(),
        Repeat { item: None::<u32>, times: 2 }.filter(|o| o.is_none()).count(),
        Repeat { item: 2, times: 3 }.collect(),
        Countdown { n: 5 }.enumerate().map(|(i, x)| i as u32 * x).max(),
    )
}

/// `PartialOrd` and `Ord` (ADR 0057): an `Ordering` is -1, 0 or 1. Derived,
/// the fields in turn: `$cmp(a.major, b.major) || $cmp(a.minor, b.minor)`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semver {
    pub major: u32,
    pub minor: u32,
}

/// `f64`s: `NaN` isn't ordered, so neither are two of these with one.
#[derive(PartialEq, PartialOrd, Clone, Copy)]
pub struct Spot {
    pub x: f64,
    pub y: f64,
}

/// By the order the variants are declared in.
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum Priority {
    Low,
    Mid,
    High,
}

/// Hand-written: shorter first, then alphabetically.
#[derive(PartialEq, Eq, Clone)]
pub struct Word(pub String);

impl PartialOrd for Word {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Word {
    fn cmp(&self, other: &Self) -> Ordering {
        let by_length = self.0.chars().count().cmp(&other.0.chars().count());
        if by_length != Ordering::Equal { by_length } else { self.0.cmp(&other.0) }
    }
}

pub fn largest<T: Ord + Copy>(xs: &[T]) -> Option<T> {
    xs.iter().copied().max()
}

pub fn in_order<T: PartialOrd>(a: &T, b: &T) -> bool {
    a <= b
}

pub fn orderings() -> (bool, bool, bool, u32, u32, Vec<u32>) {
    let a = Semver { major: 1, minor: 2 };
    let b = Semver { major: 1, minor: 10 };
    let mut all = vec![b, Semver { major: 0, minor: 9 }, a];
    all.sort();
    (
        a < b,
        a.max(b) == b,
        a.cmp(&b) == Ordering::Less,
        largest(&all).map(|v| v.minor).unwrap_or(0),
        all.iter().min().map(|v| v.minor).unwrap_or(0),
        all.iter().map(|v| v.minor).collect(),
    )
}

pub fn partial_orderings() -> (bool, bool, bool, bool) {
    let p = Spot { x: 1.0, y: f64::NAN };
    let q = Spot { x: 1.0, y: 2.0 };
    let r = Spot { x: 0.5, y: f64::NAN };
    (p < q, p >= q, r < q, p.partial_cmp(&q).is_none())
}

pub fn more_orderings() -> (bool, bool, bool, Vec<u32>, bool, u32) {
    let mut words = vec![
        Word("pear".to_string()),
        Word("fig".to_string()),
        Word("apple".to_string()),
        Word("kiwi".to_string()),
    ];
    words.sort();
    let mut priorities = vec![Priority::High, Priority::Low, Priority::Mid];
    priorities.sort();
    let mut lists = vec![vec![2u32, 1], vec![1, 5, 0], vec![1, 5]];
    lists.sort();
    let mut sizes: Vec<u32> = words.iter().map(|w| w.0.chars().count() as u32).collect();
    for list in &lists {
        sizes.push(list.len() as u32);
    }
    let mut by_key = vec![Semver { major: 2, minor: 0 }, Semver { major: 1, minor: 5 }];
    by_key.sort_by_key(|v| (v.minor, v.major));
    (
        Priority::Low < Priority::High,
        priorities[0] == Priority::Low,
        Some(3) > None,
        sizes,
        in_order(&"abc", &"abd"),
        by_key[0].major,
    )
}

/// `Debug` (ADR 0060): `{:?}` by the type, as Rust shows it. A derived one is
/// a function like any `fmt`, left out unless something shows the type.
#[derive(Debug, Clone, Copy)]
pub struct Pos {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug)]
pub struct Dims(pub u32, pub u32);

#[derive(Debug)]
pub struct Nothing;

#[derive(Debug)]
pub enum Glyph {
    Dot,
    Ring(f64),
    Box { w: u32, h: u32 },
}

#[derive(Debug)]
pub struct Six {
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub f: char,
}

#[derive(Debug)]
pub struct Boxed<T> {
    pub item: T,
}

#[derive(Debug)]
pub struct NeverShown {
    pub n: u32,
}

pub struct Hidden;

impl fmt::Debug for Hidden {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("<hidden>")
    }
}

pub fn debugged<T: fmt::Debug>(x: &T) -> String {
    format!("{x:?}")
}

pub fn debugs(n: u32) -> Vec<String> {
    let p = Pos { x: n as f64, y: -2.5 };
    let r: Result<u32, String> = if n > 1 { Ok(n) } else { Err("small".to_string()) };
    vec![
        format!("{:?}", p),
        format!("{:?} {:?} {:?}", Dims(n, 2), Nothing, Glyph::Dot),
        format!("{:?} {:?}", Glyph::Ring(1.5), Glyph::Box { w: n, h: 3 }),
        format!("{:?}", Six { a: 1, b: 2, c: 3, d: 4, e: 5, f: 'x' }),
        format!("{:?} {:?}", Boxed { item: p }, Boxed { item: Some("s") }),
        format!("{:?} {:?} {:?} {:?}", Some(1.0), None::<u32>, (n, "a", '\''), (5,)),
        format!("{:?} {:?} {:?}", vec![p, p], r, n.cmp(&1)),
        format!("{:?} {}", Hidden, debugged(&vec![Some(Dims(3, 4))])),
    ]
}

/// Generic iterators (ADR 0061). An `impl Iterator` is the type it hides; a
/// `T: Iterator` is an array or a JS iterator, so generic code takes it with
/// `Iterator.from`, and one of the crate's own is given as a JS iterator.
pub fn evens_below(n: u32) -> impl Iterator<Item = u32> {
    (0..n).filter(|x| x % 2 == 0)
}

pub fn countdown(n: u32) -> impl Iterator<Item = u32> {
    Countdown { n }
}

pub fn total<I: Iterator<Item = u32>>(items: I) -> u32 {
    items.sum()
}

pub fn middle(items: impl Iterator<Item = u32>, k: usize) -> Vec<u32> {
    items.skip(1).take(k).collect()
}

pub fn looped<I: IntoIterator<Item = u32>>(items: I) -> u32 {
    let mut sum = 0;
    for x in items {
        sum += x;
    }
    sum
}

pub fn generic_iterators(n: u32) -> (u32, Vec<u32>, Vec<u32>, u32, u32, u32) {
    (
        total(evens_below(n)),
        middle(countdown(n), 2),
        middle(evens_below(n * 2), 3),
        looped(vec![n, 2, 3]),
        total(Countdown { n }),
        countdown(n).map(|x| x + 1).sum(),
    )
}
