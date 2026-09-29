// A `&mut` kept, from rustc's tests and the refusals they had (ADR 0099):
// a user `IndexMut` giving one to a number, a method returning one to a
// field, a `&mut` in a variable kept in a struct or given as a generic
// value, all read and written through.
use std::ops::{Index, IndexMut};

struct Grid(Vec<u32>);

impl Index<usize> for Grid {
    type Output = u32;
    fn index(&self, i: usize) -> &u32 {
        &self.0[i]
    }
}

impl IndexMut<usize> for Grid {
    fn index_mut(&mut self, i: usize) -> &mut u32 {
        &mut self.0[i]
    }
}

struct Counter {
    n: i32,
}

impl Counter {
    fn get_mut(&mut self) -> &mut i32 {
        &mut self.n
    }
}

struct Holder<'a> {
    r: &'a mut i32,
}

trait Bump {
    fn bump(self);
}

impl Bump for &mut i32 {
    fn bump(self) {
        *self += 1;
    }
}

fn go<T: Bump>(t: T) {
    t.bump();
}

fn main() {
    let mut g = Grid(vec![1, 2, 3]);
    g[1] = 20;
    g[2] += 5;
    println!("{} {} {}", g[0], g[1], g[2]);

    let mut c = Counter { n: 1 };
    *c.get_mut() += 41;
    let n = c.get_mut();
    *n *= 2;
    println!("{}", c.n);

    let mut a = 1;
    let x = &mut a;
    let h = Holder { r: x };
    *h.r = 4;
    println!("{a}");

    let mut b = 1;
    let y = &mut b;
    go(y);
    println!("{b}");
}
