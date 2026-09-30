// A trait's constant, `const SIDES: u32` (ADR 0106): where the type is
// known, `Square::SIDES`, it's the value rustc computes, written in place,
// as a type's own constant is (ADR 0031); in generic code, `S::SIDES`, it's
// the impl's dictionary's, `SShape.SIDES`, a default's too.
use std::fmt::Debug;

trait Shape {
    const SIDES: u32;
    const NAME: &'static str = "shape";
    // Rustc evaluates only the constants a program uses, and `Wrap`'s would
    // divide by zero: no generic code reads it, so no dictionary has it.
    const PER_SIDE: u32 = 360 / Self::SIDES;
    fn area(&self) -> f64;
}

struct Square(f64);
struct Triangle(f64);

impl Shape for Square {
    const SIDES: u32 = 4;
    const NAME: &'static str = "square";
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}

impl Shape for Triangle {
    const SIDES: u32 = 3;
    fn area(&self) -> f64 {
        self.0 * self.0 / 2.0
    }
}

fn describe<S: Shape>(s: &S) -> String {
    format!("{} with {} sides, area {}", S::NAME, S::SIDES, s.area())
}

fn total<S: Shape>(shapes: &[S]) -> u32 {
    shapes.len() as u32 * S::SIDES
}

// A generic impl's, of a constant that doesn't need its parameter.
struct Wrap<T>(T);

impl<T> Shape for Wrap<T> {
    const SIDES: u32 = 0;
    fn area(&self) -> f64 {
        0.0
    }
}

// One of a type changed in place is a new value at each use, as a type's
// own constant is: the dictionary's is a getter, `get ZERO() { .. }`.
trait Zero {
    const ZERO: Self;
}

trait Bump {
    fn bump(&mut self);
}

#[derive(Debug)]
struct Counter {
    n: u32,
}

impl Zero for Counter {
    const ZERO: Self = Counter { n: 0 };
}

impl Bump for Counter {
    fn bump(&mut self) {
        self.n += 1;
    }
}

impl Zero for u32 {
    const ZERO: Self = 0;
}

impl Bump for u32 {
    fn bump(&mut self) {
        *self += 1;
    }
}

fn fresh<T: Zero + Bump + Debug>() -> (T, T) {
    let mut a = T::ZERO;
    a.bump();
    (a, T::ZERO)
}

fn main() {
    println!("{} {} {} {}", Square::SIDES, Triangle::NAME, <Square as Shape>::NAME, Square::PER_SIDE);
    println!("{} | {}", describe(&Square(2.0)), describe(&Triangle(3.0)));
    println!("{} {}", total(&[Square(1.0), Square(2.0)]), describe(&Wrap(1u8)));
    println!("{:?} {:?}", fresh::<Counter>(), fresh::<u32>());
}
