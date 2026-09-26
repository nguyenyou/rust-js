// The crate's own impls of std traits (ADRs 0052, 0053): `Default`,
// `From`, `Clone` and `PartialEq`, hand-written and derived. A clone is a
// copy only where it could be told apart from the value: something changes
// one of them, or a hand-written `clone` makes something else. A
// hand-written `eq` decides wherever it is, and a `fmt` returns what it
// writes.

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
