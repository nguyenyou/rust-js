// A trait object given to generic code, `T: Trait + ?Sized`: its evidence
// is Rust's built-in `impl Trait for dyn Trait`, a dictionary whose methods
// call the pair's own, as a call on the `dyn` does, and a supertrait's, a
// `dyn` of a subtrait's.
use std::fmt::Display;

trait Shape {
    fn area(&self) -> f64;
    fn scale(&mut self, k: f64);
    fn name(&self) -> String {
        "shape".to_string()
    }
}

trait Named: Shape {
    fn label(&self) -> String;
}

struct Square(f64);

impl Shape for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn scale(&mut self, k: f64) {
        self.0 *= k;
    }
}

impl Named for Square {
    fn label(&self) -> String {
        format!("square {}", self.0)
    }
}

trait Counter {
    type Item;
    fn next_item(&mut self) -> Self::Item;
}

struct Up(u32);

impl Counter for Up {
    type Item = u32;
    fn next_item(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

fn area_of<T: Shape + ?Sized>(s: &T) -> f64 {
    s.area()
}

fn grow<T: Shape + ?Sized>(s: &mut T) {
    s.scale(2.0);
}

fn describe<T: Named + ?Sized>(s: &T) -> String {
    format!("{} {} {}", s.label(), s.name(), s.area())
}

fn shout<T: Display + ?Sized>(t: &T) -> String {
    format!("{t}!")
}

fn two<C: Counter<Item = u32> + ?Sized>(c: &mut C) -> u32 {
    c.next_item() + c.next_item()
}

fn main() {
    let mut square = Square(3.0);
    let shape: &dyn Shape = &square;
    println!("{}", area_of(shape));
    {
        let shape: &mut dyn Shape = &mut square;
        grow(shape);
    }
    println!("{}", square.0);

    let named: &dyn Named = &square;
    println!("{}", describe(named));
    println!("{}", area_of(named));

    let shown: Box<dyn Display> = Box::new(5);
    println!("{}", shout(&*shown));

    let mut up = Up(0);
    let counter: &mut dyn Counter<Item = u32> = &mut up;
    println!("{}", two(counter));
    println!("{}", up.0);
}
