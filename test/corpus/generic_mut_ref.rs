// A generic `&mut T` is a box whatever `T` is (ADR 0099): generic code is
// compiled once, and its `T` may be a number. A caller whose `T` is an
// object gives it a box too, and takes the value back after. A trait's
// `&mut self` method, called through its dictionary, takes a box.
#[derive(Debug)]
struct C {
    n: i32,
}

trait Bump {
    fn bump(&mut self);
    fn get(&self) -> i32;
    fn bumped_twice(&mut self) -> i32 {
        self.bump();
        self.bump();
        self.get()
    }
}

impl Bump for i32 {
    fn bump(&mut self) {
        *self += 1;
    }
    fn get(&self) -> i32 {
        *self
    }
}

impl Bump for C {
    fn bump(&mut self) {
        self.n += 1;
    }
    fn get(&self) -> i32 {
        self.n
    }
}

fn twice<T: Bump>(x: &mut T) {
    x.bump();
    x.bump();
}

fn set<T>(x: &mut T, v: T) {
    *x = v;
}

fn bumped<T: Bump>(mut t: T) -> T {
    t.bump();
    t
}

fn main() {
    let mut n = 1;
    let mut c = C { n: 10 };
    twice(&mut n);
    twice(&mut c);
    println!("{n} {}", c.n);

    set(&mut n, 7);
    set(&mut c, C { n: 20 });
    println!("{n} {c:?}");

    println!("{} {}", bumped(1), bumped(C { n: 3 }).n);

    println!("{} {}", n.bumped_twice(), c.bumped_twice());
    println!("{n} {}", c.n);
}
