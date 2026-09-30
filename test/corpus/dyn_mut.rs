// A `&mut dyn Trait` is its pair, `{ impl, value }` (ADR 0049), and its
// `&mut self` methods are given the pair, whose `value` a box's is: an
// object's impl changes the object, and a number's the place the pair
// reads and writes (ADR 0099). A `Box<dyn Trait>` owns its pair's value.
trait Counter {
    fn bump(&mut self);
    fn get(&self) -> i32;
    fn twice(&mut self) {
        self.bump();
        self.bump();
    }
}

#[derive(Debug)]
struct C {
    n: i32,
}

impl Counter for C {
    fn bump(&mut self) {
        self.n += 1;
    }
    fn get(&self) -> i32 {
        self.n
    }
}

impl Counter for i32 {
    fn bump(&mut self) {
        *self += 10;
    }
    fn get(&self) -> i32 {
        *self
    }
}

// A `&mut dyn Sub` as a `&mut dyn Counter` is a pair on the first's
// `value`: found by probing, where the upcast wrote a copy.
trait Sub: Counter {
    fn name(&self) -> String;
}

impl Sub for i32 {
    fn name(&self) -> String {
        format!("i{self}")
    }
}

fn up(s: &mut dyn Sub) -> &mut dyn Counter {
    s
}

fn use_it(c: &mut dyn Counter) -> i32 {
    c.bump();
    c.twice();
    c.get()
}

fn main() {
    let mut c = C { n: 1 };
    let mut n = 5;
    println!("{} {}", use_it(&mut c), use_it(&mut n));
    println!("{c:?} {n}");

    let d: &mut dyn Counter = &mut n;
    d.bump();
    let shared: &dyn Counter = d;
    println!("{} {n}", shared.get());
    let s: &mut dyn Sub = &mut n;
    up(s).bump();
    println!("{} {n}", s.name());

    let mut boxed: Box<dyn Counter> = Box::new(7);
    boxed.bump();
    let mut list: Vec<Box<dyn Counter>> = vec![Box::new(C { n: 0 }), Box::new(1)];
    for b in list.iter_mut() {
        b.twice();
    }
    println!("{} {} {}", boxed.get(), list[0].get(), list[1].get());
}
