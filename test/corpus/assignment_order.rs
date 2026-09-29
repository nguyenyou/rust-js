//@ run-fail: index out of bounds: the len is 0 but the index is 0
// `place = value` runs the value, then the place, and so does `place op=
// value` of primitives; one that calls `AddAssign` runs the place first. A
// place that panics, an index out of bounds, panics only after the value.
// Found by a generated program (seed 1476).
use std::ops::AddAssign;

struct P {
    x: i32,
}

#[derive(Debug)]
struct Total(i32);

impl AddAssign<i32> for Total {
    fn add_assign(&mut self, other: i32) {
        self.0 += other;
    }
}

fn index(name: &str, i: usize) -> usize {
    println!("{name}: index");
    i
}

fn value(name: &str, x: i32) -> i32 {
    println!("{name}: value");
    x
}

struct Counter {
    n: i32,
}

fn bump(c: &mut Counter) -> usize {
    c.n += 1;
    0
}

fn reset(c: &mut Counter) -> i32 {
    c.n = 10;
    2
}

fn set(r: &mut [i32; 2]) {
    r[index("through a reference", 0)] = value("through a reference", 8);
}

fn main() {
    let mut v = vec![0, 0];
    v[index("vec =", 0)] = value("vec =", 1);
    v[index("vec +=", 1)] += value("vec +=", 2);
    let mut a = [0, 0];
    a[index("array =", 0)] = value("array =", 3);
    a[index("array +=", 1)] += value("array +=", 4);
    let mut ps = vec![P { x: 0 }];
    ps[index("field =", 0)].x = value("field =", 5);
    ps[index("field +=", 0)].x += value("field +=", 6);
    let mut totals = vec![Total(0)];
    totals[index("AddAssign", 0)] += value("AddAssign", 7);
    set(&mut a);
    let mut rows = vec![vec![0, 0]];
    rows[index("rows row", 0)][index("rows column", 1)] = value("rows", 9);
    println!("{v:?} {a:?} {} {:?} {rows:?}", ps[0].x, totals[0]);
    // The value is read before the place changes it. Found in review.
    let mut x = 1;
    let mut one = [0];
    one[{ x = 2; 0 }] = x;
    let mut y = 1;
    let mut w = vec![0];
    w[{ y = 2; 0 }] = y;
    let mut z = 1;
    let mut u = vec![0];
    u[{ z = 2; 0 }] += z;
    let mut c = Counter { n: 1 };
    let mut t = vec![0];
    t[bump(&mut c)] = c.n;
    println!("{} {x} {} {y} {} {z} {} {}", one[0], w[0], u[0], t[0], c.n);
    // `+=` reads its place after the value, which may change it. Found in
    // review.
    let mut d = Counter { n: 1 };
    d.n += reset(&mut d);
    let mut e = 1;
    let mut inc = || {
        e += 10;
        2
    };
    e += inc();
    println!("{} {e}", d.n);
    // A `Vec`'s index is `index_mut(&mut *cur, i)`, which takes `*cur`
    // before `i` runs; an array's is the place `(*cur)[i]`, which follows
    // `cur` after. Found in review.
    let (mut a, mut b) = (vec![1, 2], vec![10, 20]);
    let mut cur = &mut a;
    cur[{
        cur = &mut b;
        0
    }] += 1;
    println!("{a:?} {b:?}");
    let (mut a, mut b) = (vec![1, 2], vec![10, 20]);
    let mut cur = &mut a;
    cur[{
        cur = &mut b;
        0
    }] = 7;
    println!("{a:?} {b:?}");
    let (mut a, mut b) = (vec![1, 2], vec![10, 20]);
    let mut cur = &mut a;
    let x = &mut cur[{
        cur = &mut b;
        0
    }];
    *x += 1;
    println!("{a:?} {b:?}");
    let (mut a, mut b) = ([1, 2], [10, 20]);
    let mut cur = &mut a;
    let x = &mut cur[{
        cur = &mut b;
        0
    }];
    *x += 1;
    println!("{a:?} {b:?}");
    let mut empty: Vec<i8> = Vec::new();
    empty[index("out of bounds", 0)] = value("out of bounds", 8) as i8;
}
