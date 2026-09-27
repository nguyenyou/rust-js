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
    let mut empty: Vec<i8> = Vec::new();
    empty[index("out of bounds", 0)] = value("out of bounds", 8) as i8;
}
