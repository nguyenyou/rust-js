// A const parameter is a value its caller gives, as a dictionary is (ADR
// 0107): `sum(values, N)`, its value where it's known, `3`, and in generic
// code its own `N`.
use std::fmt::{self, Debug};

fn sum<const N: usize>(values: [u32; N]) -> u32 {
    let mut total = 0;
    for i in 0..N {
        total += values[i];
    }
    total
}

// Given on to another, and with a type's dictionary after it.
fn describe<T: Debug, const N: usize>(values: [T; N]) -> String {
    format!("{N} of {values:?}, {}", count::<N>())
}

fn count<const N: usize>() -> usize {
    N * 2
}

// `[x; N]` of a caller's `N`.
fn filled<const N: usize>(x: u8) -> [u8; N] {
    [x; N]
}

// A type's, which its methods are given where it's known, from the receiver.
struct Ring<const CAP: usize> {
    items: Vec<u32>,
}

impl<const CAP: usize> Ring<CAP> {
    fn new() -> Self {
        Ring { items: Vec::new() }
    }

    fn push(&mut self, x: u32) {
        if self.items.len() == CAP {
            self.items.remove(0);
        }
        self.items.push(x);
    }

    fn capacity(&self) -> usize {
        CAP
    }
}

// Of other types: a `bool` and a `char`.
fn flag<const ON: bool, const C: char>() -> String {
    if ON { C.to_string() } else { "-".to_string() }
}

// A closure sees its function's.
fn scaled<const K: u32>(values: &[u32]) -> Vec<u32> {
    values.iter().map(|v| v * K).collect()
}

// An impl's: its dictionary is given it, one for each value, `ringShow(2)`.
trait Show {
    fn show(&self) -> String;
}

impl<const CAP: usize> Show for Ring<CAP> {
    fn show(&self) -> String {
        format!("{}/{}", self.items.len(), CAP)
    }
}

impl<const CAP: usize> fmt::Display for Ring<CAP> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ring of {}", self.capacity())
    }
}

// Of two, a `Map` of `Map`s.
struct Grid<const W: usize, const H: usize>;

impl<const W: usize, const H: usize> Show for Grid<W, H> {
    fn show(&self) -> String {
        format!("{W}x{H}")
    }
}

fn shown<S: Show>(s: &S) -> String {
    s.show()
}

fn main() {
    println!("{} {}", sum([1, 2, 3]), sum([10; 4]));
    println!("{}", describe(["a", "b"]));
    println!("{:?} {:?}", filled::<3>(7), filled::<0>(1));
    let mut ring: Ring<2> = Ring::new();
    for x in 1..=5 {
        ring.push(x);
    }
    println!("{:?} {}", ring.items, ring.capacity());
    println!("{} {}", flag::<true, 'x'>(), flag::<false, 'y'>());
    println!("{:?}", scaled::<3>(&[1, 2]));
    let big: Ring<5> = Ring::new();
    println!("{} {} {} {}", shown(&ring), shown(&big), shown(&ring), ring);
    let all: Vec<Box<dyn Show>> = vec![Box::new(ring), Box::new(big)];
    println!("{:?}", all.iter().map(|s| s.show()).collect::<Vec<_>>());
    println!("{} {} {}", shown(&Grid::<2, 3>), shown(&Grid::<2, 4>), shown(&Grid::<2, 3>));
    // A function as a value, given its `N`.
    let twice = count::<4>;
    println!("{}", twice());
}
