// Const generics of traits and of their methods (ADR 0135): a trait's const
// parameter is part of which trait an impl is, and a method's is given its
// value, as a function's is (ADR 0107), through a dictionary too.

trait Repeat {
    fn repeat<const N: usize>(&self) -> [u32; N];
    fn twice(&self) -> [u32; 2] {
        self.repeat::<2>()
    }
}

struct Seven;

impl Repeat for Seven {
    fn repeat<const N: usize>(&self) -> [u32; N] {
        [7; N]
    }
}

fn three<R: Repeat>(r: &R) -> [u32; 3] {
    r.repeat::<3>()
}

trait Scaled<const K: u32> {
    fn scaled(&self) -> u32;
    fn factor(&self) -> u32 {
        K
    }
}

struct Meters(u32);

impl Scaled<10> for Meters {
    fn scaled(&self) -> u32 {
        self.0 * 10
    }
}

impl Scaled<100> for Meters {
    fn scaled(&self) -> u32 {
        self.0 * 100
    }
}

struct Any(u32);

impl<const K: u32> Scaled<K> for Any {
    fn scaled(&self) -> u32 {
        self.0 * K
    }
}

// A const parameter and a type bound of a method's own, both given by its
// caller through the dictionary.
trait Pad {
    fn pad<const N: usize, T: std::fmt::Display>(&self, value: T) -> String;
}

struct Dots;

impl Pad for Dots {
    fn pad<const N: usize, T: std::fmt::Display>(&self, value: T) -> String {
        format!("{}{}", ".".repeat(N), value)
    }
}

fn padded<P: Pad>(p: &P) -> String {
    p.pad::<3, _>(42)
}

fn by_ten<S: Scaled<10>>(s: &S) -> u32 {
    s.scaled() + s.factor()
}

fn by<const K: u32, S: Scaled<K>>(s: &S) -> (u32, u32) {
    (s.scaled(), s.factor())
}

fn main() {
    println!("{:?} {:?} {:?}", Seven.repeat::<4>(), Seven.twice(), three(&Seven));
    let m = Meters(3);
    println!(
        "{} {} {} {}",
        <Meters as Scaled<10>>::scaled(&m),
        <Meters as Scaled<100>>::scaled(&m),
        <Meters as Scaled<100>>::factor(&m),
        by_ten(&m)
    );
    println!("{:?} {:?} {:?}", by::<100, _>(&m), by::<5, _>(&Any(2)), by::<7, _>(&Any(3)));
    println!("{} {}", padded(&Dots), Dots.pad::<1, _>("x"));
}
