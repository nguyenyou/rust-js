// A `Formatter`'s options are handed on (ADR 0058): a `fmt` that gives its
// `Formatter` to another `fmt` gives it the placeholder's width, precision
// and sign, as a generic `T`'s, a `dyn`'s, a derived `Debug`'s and a
// builder's do.
use std::fmt;

struct Meters(f64);

impl fmt::Display for Meters {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)?;
        f.write_str(" m")
    }
}

struct Name(&'static str);

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.0, f)
    }
}

#[derive(Debug)]
struct Point {
    x: i32,
    y: f64,
}

struct Pair(u8, &'static str);

impl fmt::Debug for Pair {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Pair").field(&self.0).field(&self.1).finish()
    }
}

fn show<T: fmt::Display>(value: T) -> String {
    format!("[{:>6}]", value)
}

fn debug<T: fmt::Debug>(value: T) -> String {
    format!("[{:<4?}]", value)
}

fn main() {
    println!("[{:8.2}] [{:+}] [{:<9.1}]", Meters(1.5), Meters(2.0), Meters(-0.25));
    println!("{} {} {} {}", show(42), show("ab"), show(Meters(3.0)), show(Name("xyz")));
    let d: &dyn fmt::Display = &7;
    println!("[{:<4}] [{:^7}] [{:.2}]", d, Name("rust"), Name("rust"));
    println!("[{:5?}] [{:.1?}]", Point { x: 1, y: 2.0 }, Some(Point { x: -3, y: 0.25 }));
    println!("{:#5?}", Point { x: 4, y: 5.5 });
    println!("{}", Meters(0.5));
    println!("[{:4?}] {} {}", Pair(7, "a"), debug(1), debug(Pair(2, "b")));
    let b: Box<dyn fmt::Display> = Box::new(Meters(-2.5));
    println!("[{:+08.1}] [{:*^12}] [{:>5}]", b, Meters(1.0), true);
}
