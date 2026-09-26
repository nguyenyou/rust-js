// `const` items (ADR 0031): rustc works out each value at compile time, and
// the JS gives it a name: `const SIZE = 4096;`.

pub const SIZE: u32 = 4 * 1024;
const GREETING: &str = "hello";
const RATIO: f64 = 1.0 / 4.0;
const ON: bool = !false;
const ORIGIN: Point = Point { x: 0, y: 0 };
const PAIR: (i32, u32) = (-3, SIZE / 2);
const PRIMES: [u32; 4] = [2, 3, 5, 7];
const NOTHING: Option<i32> = None;
const LEVEL: Level = Level::High;

#[derive(Clone, Copy, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Level {
    Low,
    High,
}

pub fn size_in_kb() -> u32 {
    SIZE / 1024
}

pub fn greeting() -> String {
    GREETING.to_string() + "!"
}

pub fn quarter(x: f64) -> f64 {
    x * RATIO
}

pub fn on() -> bool {
    ON
}

/// Each use of a `const` is a value of its own: changing one leaves the
/// next use as it was.
pub fn moved(dx: i32) -> (Point, Point) {
    let mut p = ORIGIN;
    p.x += dx;
    (p, ORIGIN)
}

pub fn pair() -> (i32, u32) {
    PAIR
}

pub fn prime_sum() -> u32 {
    let mut sum = 0;
    for p in PRIMES {
        sum += p;
    }
    sum
}

pub fn nothing() -> Option<i32> {
    NOTHING
}

pub fn high() -> bool {
    LEVEL == Level::High && LEVEL != Level::Low
}

/// Constants from std are written in place.
pub fn limits() -> (u32, i32) {
    (u32::MAX, i32::MIN)
}

/// A `const` inside a function.
pub fn local() -> u32 {
    const STEP: u32 = 3;
    STEP * 2
}
