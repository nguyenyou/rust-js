// A trait's generic method, `describe<T: Display>` (ADR 0106): called where
// the impl is known, it's the impl's method, given `T`'s evidence; called
// through a dictionary, the evidence comes after the arguments, in the
// order the trait declares it, and the dictionary's entry passes it on, in
// the impl's.
use std::fmt::{Debug, Display};

trait Shape {
    fn area(&self) -> f64;
    // A default, copied into each impl.
    fn describe<T: Display>(&self, label: T) -> String {
        format!("{label}: {}", self.area())
    }
    // A bound that's a closure's, which needs no dictionary.
    fn scaled<F: Fn(f64) -> f64>(&self, f: F) -> f64;
    // Each impl's own, with two bounds, which one impl writes the other way
    // round.
    fn tagged<A: Clone + Debug, B: Display>(&self, a: A, b: B) -> String;
}

struct Circle(f64);
struct Square(f64);

impl Shape for Circle {
    fn area(&self) -> f64 {
        3.0 * self.0 * self.0
    }
    fn scaled<F: Fn(f64) -> f64>(&self, f: F) -> f64 {
        f(self.area())
    }
    fn tagged<A: Clone + Debug, B: Display>(&self, a: A, b: B) -> String {
        format!("circle {:?} {b} {:?}", a.clone(), a)
    }
}

impl Shape for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn describe<T: Display>(&self, label: T) -> String {
        format!("square {label} of {}", self.area())
    }
    fn scaled<F: Fn(f64) -> f64>(&self, f: F) -> f64 {
        f(self.0)
    }
    fn tagged<A: Debug + Clone, B: Display>(&self, a: A, b: B) -> String
    where
        A: Clone,
    {
        format!("square {b} {:?}", a)
    }
}

fn report<S: Shape>(s: &S) -> String {
    s.describe("area")
}

fn everything<S: Shape>(s: &S) -> (String, f64, String) {
    (s.describe(7u32), s.scaled(|a| a * 2.0), s.tagged(vec![1, 2], 'x'))
}

fn main() {
    let c = Circle(1.0);
    let s = Square(2.0);
    println!("{} | {} | {}", c.describe("c"), s.describe(3.5), c.tagged("a", 1u8));
    println!("{} | {}", report(&c), report(&s));
    println!("{:?}", everything(&c));
    println!("{:?}", everything(&s));
}
