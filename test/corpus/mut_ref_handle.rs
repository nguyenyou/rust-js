// A `&mut` kept anywhere but a variable used for a few lines is a handle
// (ADR 0099): something with a box's `value` that reads and writes its
// place. In a struct, written through as it's dropped; chosen by a branch;
// in a variable assigned again; in an `Option` and a `Vec`; returned.
struct Counter<'a>(&'a mut i32);

impl Drop for Counter<'_> {
    fn drop(&mut self) {
        *self.0 += 1;
    }
}

struct Pair<'a> {
    left: &'a mut i32,
    right: &'a mut String,
}

fn pick<'a>(take_first: bool, a: &'a mut i32, b: &'a mut i32) -> &'a mut i32 {
    if take_first { a } else { b }
}

fn first(v: &mut Vec<i32>) -> &mut i32 {
    &mut v[0]
}

fn main() {
    let mut drops = 0;
    for _ in 0..2 {
        let _c = Counter(&mut drops);
    }
    println!("{drops}");

    let mut n = 1;
    let mut s = String::from("a");
    let p = Pair { left: &mut n, right: &mut s };
    *p.left += 10;
    p.right.push('b');
    // Formatted, a `&mut` kept shows what it points at.
    println!("{} {:?}", p.left, p.right);
    println!("{n} {s}");

    let (mut a, mut b) = (1, 2);
    for take in [true, false] {
        let r = if take { &mut a } else { &mut b };
        *r *= 10;
    }
    println!("{a} {b}");

    let (mut x, mut y) = (0, 0);
    let mut cur = &mut x;
    *cur += 1;
    cur = &mut y;
    *cur += 5;
    println!("{cur}");
    println!("{x} {y}");

    let mut m = 3;
    let o: Option<&mut i32> = Some(&mut m);
    if let Some(r) = o {
        *r += 1;
    }
    println!("{m}");

    let (mut i, mut j) = (1, 2);
    let refs: Vec<&mut i32> = vec![&mut i, &mut j];
    for r in refs {
        *r *= 7;
    }
    println!("{i} {j}");

    let (mut u, mut w) = (5, 6);
    *pick(false, &mut u, &mut w) = 60;
    let got = pick(true, &mut u, &mut w);
    *got += 1;
    println!("{u} {w}");

    let mut v = vec![1, 2];
    *first(&mut v) = 9;
    println!("{v:?}");
}
