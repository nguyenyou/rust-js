// A trait object of a value with a destructor (ADR 0098): its dictionary
// carries its drop, as Rust's vtable does, and dropping the `dyn` runs it.
// A box's value, `*b`, is borrowed through, moved out of it, or put back.
use std::fmt::Debug;

struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

trait Shape {
    fn name(&self) -> String;
}

impl Shape for Noisy {
    fn name(&self) -> String {
        format!("noisy {}", self.0)
    }
}

struct Quiet(u8);

impl Shape for Quiet {
    fn name(&self) -> String {
        format!("quiet {}", self.0)
    }
}

struct Wrap<T>(T);

impl<T: Debug> Shape for Wrap<T> {
    fn name(&self) -> String {
        format!("wrap {:?}", self.0)
    }
}

#[derive(Debug)]
struct Loud(u8);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop loud {}", self.0);
    }
}

struct Holder {
    shape: Box<dyn Shape>,
}

fn consume(shape: Box<dyn Shape>) -> String {
    shape.name()
}

fn main() {
    {
        let one: Box<dyn Shape> = Box::new(Noisy("one"));
        println!("{}", one.name());
    }
    println!("after one");

    let all: Vec<Box<dyn Shape>> = vec![Box::new(Noisy("a")), Box::new(Quiet(1)), Box::new(Wrap(Loud(2))), Box::new(Noisy("b"))];
    for shape in &all {
        println!("{}", shape.name());
    }
    drop(all);
    println!("after all");

    let lent = Noisy("lent");
    let borrowed: &dyn Shape = &lent;
    println!("{}", borrowed.name());
    let loud = Loud(3);
    let shown: &dyn Debug = &loud;
    println!("{shown:?}");

    let holder = Holder { shape: Box::new(Noisy("held")) };
    println!("{}", holder.shape.name());
    println!("{}", consume(Box::new(Noisy("given"))));

    {
        let kept = Box::new(Noisy("kept"));
        println!("{}", kept.name());
    }
    let mut boxed = Box::new(Noisy("boxed"));
    println!("{}", boxed.name());
    let unboxed = *boxed;
    println!("moved {}", unboxed.0);
    *boxed = Noisy("refilled");
    println!("{}", boxed.name());
    println!("end");
}
