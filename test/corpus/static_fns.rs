// A static or a constant whose value is a function, a closure or a `dyn`,
// which rustc's value of it can't say, is its initializer (ADR 0096): it
// can't change in place, so the one JS value is every use's.
use std::fmt::Display;

trait Greet {
    fn hi(&self) -> String;
}

struct En;

impl Greet for En {
    fn hi(&self) -> String {
        "hello".to_string()
    }
}

fn double(x: i32) -> i32 {
    x * 2
}

fn triple(x: i32) -> i32 {
    x * 3
}

static F: fn(i32) -> i32 = double;
const G: fn(i32) -> i32 = |x| x + 1;
static TABLE: &[(&str, fn(i32) -> i32)] = &[("double", double), ("triple", triple)];
const MAYBE: Option<fn(i32) -> i32> = Some(triple);
static GREETER: &(dyn Greet + Sync) = &En;
const SHOWN: &dyn Display = &42;
const ADD: &(dyn Fn(i32) -> i32 + Sync) = &|x| x + 10;

fn main() {
    println!("{} {}", F(2), G(2));
    for (name, f) in TABLE {
        println!("{name} {}", f(5));
    }
    if let Some(f) = MAYBE {
        println!("{}", f(1));
    }
    println!("{} {} {}", GREETER.hi(), SHOWN, ADD(1));
}
