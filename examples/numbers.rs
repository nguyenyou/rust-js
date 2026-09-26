// Numbers' methods, operators on the crate's own types, and `vec![x; n]`
// (ADR 0064). A number is a JS number, so most methods are `Math`'s:
// `x.sqrt()` is `Math.sqrt(x)`. Where JS answers differently (`round` of a
// half, `pow` past 2^53), a helper gives Rust's answer.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    // A type's own constant is its value where it's used, as Rust's is.
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn len(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}

pub fn vectors() -> String {
    let a = Vec2::new(3.0, 4.0);
    let b = Vec2::new(1.0, -2.0);
    let mut c = a + b * 2.0 - -a + Vec2::ZERO;
    c += b;
    format!("{:?} {} {:?} {}", c, a.len(), (a - b).len(), Vec2::ZERO.y)
}

pub fn integers(a: i32, b: u32) -> String {
    format!(
        "{} {} {} {} {:?} {:?} {:?} {:?} {} {} {} {} {} {} {} {} {} {}",
        a.abs(),
        a.pow(2),
        a.rem_euclid(7),
        a.div_euclid(7),
        b.checked_sub(10),
        a.checked_add(i32::MAX),
        a.checked_mul(a),
        a.checked_div(b as i32 - 3),
        b.saturating_sub(100),
        b.saturating_add(u32::MAX - 5),
        a.saturating_mul(1 << 20),
        b.wrapping_sub(5),
        a.wrapping_mul(1 << 30),
        a.signum(),
        b.leading_zeros(),
        b.trailing_zeros(),
        a.count_ones(),
        b.abs_diff(100),
    )
}

pub fn narrow(a: i8, b: u8) -> String {
    format!(
        "{} {} {} {} {} {} {} {}",
        a.pow(3),
        b.pow(3),
        a.abs(),
        a.leading_zeros(),
        a.count_ones(),
        b.trailing_zeros(),
        b.is_power_of_two(),
        a.rem_euclid(-3),
    )
}

pub fn floats(x: f64) -> String {
    format!(
        "{} {} {} {} {} {} {:.3} {} {} {} {} {}",
        x.floor(),
        x.ceil(),
        x.round(),
        x.trunc(),
        x.abs(),
        x.max(1.5),
        x.powf(0.5),
        x.powi(3),
        x.powi(-2),
        x.is_nan(),
        x.is_finite(),
        x.hypot(4.0),
    )
}

pub fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[derive(Debug, Clone)]
pub struct Cell2 {
    pub hits: u32,
}

pub fn grids(n: usize) -> (Vec<Vec<u32>>, Vec<u32>, String) {
    let mut m = vec![vec![0u32; n]; n];
    for i in 0..n {
        for j in 0..n {
            m[i][j] = (i * n + j) as u32;
        }
    }
    let mut t = vec![vec![0u32; n]; n];
    for (i, row) in m.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            t[j][i] = v;
        }
    }
    // Each is its own copy: changing one leaves the others.
    let cell = Cell2 { hits: 0 };
    let mut cells = vec![cell; 3];
    cells[1].hits += 5;
    let flags = vec![1u32; n];
    (t, flags, format!("{:?}", cells))
}

pub fn searches() -> String {
    let v = [1, 3, 3, 3, 5, 7, 7];
    let found: Vec<String> = [0, 1, 3, 4, 7, 8]
        .iter()
        .map(|x| format!("{:?}", v.binary_search(x)))
        .collect();
    let empty: [u32; 0] = [];
    format!("{} {:?}", found.join(" "), empty.binary_search(&1))
}

// Integers are never -0, which `%`, `/` and `*` can make in JS: `1 / x` tells.
pub fn zeros(a: i32) -> Vec<f64> {
    let parts = [
        a.rem_euclid(7),
        a.div_euclid(-9),
        a.checked_mul(-3).unwrap_or(1),
        a.saturating_mul(-4),
        a.checked_div(-8).unwrap_or(1),
    ];
    parts.iter().map(|&p| 1.0 / p as f64).collect()
}

pub fn panics(i: u32) -> i32 {
    let d = i as i32 - 1;
    [7i32, i32::MIN][i as usize % 2].rem_euclid(d)
}

pub fn report() -> String {
    let mut out = vectors() + "\n";
    for (a, b) in [
        (-12, 3u32),
        (7, 200),
        (i32::MIN, 0),
        (0, u32::MAX),
        (-1, 1 << 31),
        (-14, 1),
    ] {
        out += &format!("{}\n", integers(a, b));
    }
    for (a, b) in [(-5i8, 7u8), (127, 128), (-128, 0), (6, 64)] {
        out += &format!("{}\n", narrow(a, b));
    }
    for x in [
        2.5,
        -2.5,
        0.49999999999999994,
        -1.25,
        9.0,
        -0.0,
        1e21,
        f64::NAN,
        f64::INFINITY,
    ] {
        out += &format!("{}\n", floats(x));
    }
    out += &format!("{} {} {}\n", gcd(48, 18), f64::EPSILON, 0.1 + 0.2);
    out += &format!("{:?} {:?}\n", zeros(-14), zeros(0));
    out += &format!("{:?}\n{}\n", grids(3), searches());
    out
}
