// An operator in generic code, `a + b` of a `T: Add`, is its dictionary's
// (ADR 0108): a number's is its own `+`, as `a + b` of one is, and a type
// of the crate's own its impl's `add`.
use std::ops::{Add, Div, Mul, Neg, Not, Rem, Sub};

fn total<T: Add<Output = T> + Copy>(values: &[T], zero: T) -> T {
    let mut sum = zero;
    for &v in values {
        sum = sum + v;
    }
    sum
}

fn affine<T>(x: T, a: T, b: T) -> T
where
    T: Mul<Output = T> + Add<Output = T>,
{
    a * x + b
}

fn parts<T: Div<Output = T> + Rem<Output = T> + Sub<Output = T> + Copy>(a: T, b: T) -> (T, T, T) {
    (a / b, a % b, a - b)
}

fn flip<T: Neg<Output = T>>(x: T) -> T {
    -x
}

fn invert<T: Not<Output = T>>(x: T) -> T {
    !x
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct V2 {
    x: f64,
    y: f64,
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2 { x: self.x + o.x, y: self.y + o.y }
    }
}

impl Mul for V2 {
    type Output = V2;
    fn mul(self, o: V2) -> V2 {
        V2 { x: self.x * o.x, y: self.y * o.y }
    }
}

impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        V2 { x: -self.x, y: -self.y }
    }
}

// Of another right side, and another output.
struct Meters(f64);

impl Mul<f64> for Meters {
    type Output = Meters;
    fn mul(self, k: f64) -> Meters {
        Meters(self.0 * k)
    }
}

// A number's, of the crate's own on its right: the crate's impl.
impl Mul<V2> for f64 {
    type Output = V2;
    fn mul(self, v: V2) -> V2 {
        V2 { x: self * v.x, y: self * v.y }
    }
}

fn times<K: Mul<V2, Output = V2>>(k: K, v: V2) -> V2 {
    k * v
}

fn scaled<T: Mul<f64, Output = T>>(x: T, k: f64) -> T {
    x * k
}

fn main() {
    // `u8`'s and `i32`'s wrap as they do outside generic code.
    println!("{} {} {}", total(&[1u32, 2, 3], 0), total(&[1.5, 2.25], 0.0), total(&[200u8, 50], 5));
    println!("{} {}", affine(2i32, -3, 7), affine(0.5f64, 4.0, 1.0));
    println!("{:?} {:?}", parts(17i64, 5), parts(-7i32, 2));
    println!("{} {} {}", flip(5i32), flip(2.5f64), invert(true));
    println!("{} {}", invert(0u8), invert(5i32));
    let v = V2 { x: 1.0, y: 2.0 };
    println!("{:?}", total(&[v, v, V2 { x: 0.5, y: 0.0 }], V2 { x: 0.0, y: 0.0 }));
    println!("{:?} {:?}", affine(v, v, v), flip(v));
    println!("{} {:?}", scaled(Meters(2.0), 1.5).0, times(3.0, v));
}
