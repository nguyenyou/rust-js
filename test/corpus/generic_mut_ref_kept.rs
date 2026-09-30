// A generic `&mut T` kept or returned is a cell, a box or a handle, as a
// `&mut` to a number is (ADR 0099): generic code is compiled once, and its
// `T` may be a number. A caller whose `T` is a number is given the cell; one
// whose `T` is an object, what's in it, the object, as its own `&mut` to one
// is.
#[derive(Debug)]
struct C {
    n: i32,
}

struct List<T> {
    value: T,
    next: Option<Box<List<T>>>,
}

fn to_refs<T>(mut list: &mut List<T>) -> Vec<&mut T> {
    let mut result = vec![];
    loop {
        result.push(&mut list.value);
        if let Some(n) = list.next.as_mut() {
            list = n;
        } else {
            return result;
        }
    }
}

fn set_all<T: Clone>(refs: Vec<&mut T>, v: T) {
    for r in refs {
        *r = v.clone();
    }
}

fn pick<'a, T>(c: bool, a: &'a mut T, b: &'a mut T) -> &'a mut T {
    if c { a } else { b }
}

fn first<T>(v: &mut Vec<T>) -> &mut T {
    &mut v[0]
}

// A `DerefMut` of the crate's, `checked.push(1)`: its `&mut T`, of a `Vec`,
// is the `Vec`, and of a number, a cell. Found by rustc's `issue-42463.rs`.
struct Checked<T> {
    value: T,
}

impl<T> std::ops::Deref for Checked<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Checked<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

// A trait method of the crate's giving cells, matched: its binding is a
// cell, not a std call's item.
trait Slot {
    fn slot(&mut self) -> Option<&mut i32>;
}

impl Slot for C {
    fn slot(&mut self) -> Option<&mut i32> {
        Some(&mut self.n)
    }
}

fn main() {
    let mut list = List { value: 1, next: Some(Box::new(List { value: 2, next: None })) };
    let refs = to_refs(&mut list);
    println!("{:?} {}", refs, refs == vec![&mut 1, &mut 2]);
    set_all(refs, 7);
    let second = match &list.next {
        Some(n) => n.value,
        None => 0,
    };
    println!("{} {second}", list.value);

    let (mut a, mut b) = (1, 2);
    *pick(true, &mut a, &mut b) += 10;
    *pick(false, &mut a, &mut b) += 20;
    let r = pick(true, &mut a, &mut b);
    *r *= 2;
    println!("{a} {b}");

    let mut s = (String::from("x"), String::from("y"));
    pick(false, &mut s.0, &mut s.1).push('!');
    println!("{:?}", s);

    let (mut c, mut d) = (C { n: 1 }, C { n: 2 });
    pick(true, &mut c, &mut d).n += 5;
    let e = pick(false, &mut c, &mut d);
    e.n *= 10;
    println!("{c:?} {d:?}");

    let mut v = vec![C { n: 3 }];
    first(&mut v).n += 1;
    let mut w = vec![4, 5];
    *first(&mut w) += 1;
    println!("{v:?} {w:?}");

    let mut checked = Checked { value: vec![0] };
    checked.push(1);
    let mut count = Checked { value: 1 };
    *count += 1;
    println!("{:?} {}", *checked, *count);

    let mut k = C { n: 1 };
    match k.slot() {
        Some(r) => *r += 1,
        None => {}
    }
    println!("{k:?}");
}
