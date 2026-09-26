// The crate's own impls of std traits (ADR 0052): `Default`, `From` and
// `Clone`, hand-written and derived. A clone is a copy only where it could
// be told apart from the value: something changes one of them, or a
// hand-written `clone` makes something else.

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
