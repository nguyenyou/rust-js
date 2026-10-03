// A bound is found however it's written (ADR 0049): `<I as Int>::T: NonZero`
// is `J: NonZero` of an `I: Int<T = J>`, a supertrait's arguments are what
// they normalize to, `T: ToString` is a dictionary of its own, and a
// default copied into a generic impl calls the impl's methods.
use std::fmt;

trait Int {
    type T;
    fn value(&self) -> u32;
}

trait NonZero {
    fn non_zero(&self) -> bool;
}

struct Small(u32);

impl Int for Small {
    type T = u8;
    fn value(&self) -> u32 {
        self.0
    }
}

impl NonZero for u8 {
    fn non_zero(&self) -> bool {
        *self != 0
    }
}

fn check<I: Int<T = J>, J>(i: I, j: J) -> bool
where
    <I as Int>::T: NonZero,
{
    i.value() > 0 && j.non_zero()
}

trait Source: Produce<<Self as Source>::Item> {
    type Item;
}

trait Produce<T> {
    fn produce(&self) -> T;
}

impl Source for i32 {
    type Item = u32;
}

impl Produce<u32> for i32 {
    fn produce(&self) -> u32 {
        *self as u32 * 2
    }
}

fn produced<T: Source<Item = u32>>(t: &T) -> u32 {
    t.produce()
}

struct Celsius(f64);

impl fmt::Display for Celsius {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}°C", self.0)
    }
}

fn shout<T: ToString>(x: T) -> String {
    x.to_string().to_uppercase()
}

trait Getter<T: Clone> {
    fn get(&self) -> T;
    fn twice(&self) -> (T, T) {
        let x = self.get();
        (x.clone(), x)
    }
}

impl<T: Clone> Getter<T> for Option<T> {
    fn get(&self) -> T {
        self.as_ref().unwrap().clone()
    }
}

trait Digits: Sized {
    type Iter: Iterator<Item = u8>;
    fn digit_iter(self) -> Self::Iter;
    fn digit_sum(self) -> u32 {
        self.digit_iter().map(|d: u8| d as u32).fold(0, |s, d| s + d)
    }
}

impl<I> Digits for I
where
    I: Iterator<Item = u8>,
{
    type Iter = I;
    fn digit_iter(self) -> I {
        self
    }
}

fn main() {
    println!("{} {}", check(Small(3), 1u8), check(Small(3), 0u8));
    println!("{}", produced(&21));
    println!("{} {} {}", shout(12), shout("abc"), shout(Celsius(21.5)));
    println!("{:?} {:?}", Some(4).twice(), Some("hi".to_string()).twice());
    println!("{}", vec![1u8, 2, 3].into_iter().digit_sum());
}
