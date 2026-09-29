// `assert_eq!(*o, 3)` of a `&mut` parameter that's a box (ADR 0074): what
// it compares is `*o`, the box's `value`, not the box. `assert_eq!`, a
// `match (a, b)` and `format_args!` take a tuple's parts where they are,
// looking through `&` but not through the `*` of a box. Found by rustc's
// `regions/regions-lifetime-static-items-enclosing-scopes.rs`.
fn number(o: &mut i32) {
    assert_eq!(*o, 3);
    *o += 1;
}

fn text(o: &mut String) {
    assert_eq!(*o, "a");
    o.push('b');
}

fn generic<T: PartialEq + std::fmt::Debug>(o: &mut Option<T>) {
    assert_eq!(*o, None);
}

fn pair(o: &mut i32, p: &mut i32) -> i32 {
    match (&*o, &*p) {
        (a, b) if a < b => *b - *a,
        (a, b) => *a - *b,
    }
}

fn main() {
    let mut x = 3;
    number(&mut x);
    let mut s = String::from("a");
    text(&mut s);
    let mut n: Option<isize> = None;
    generic(&mut n);
    generic::<isize>(&mut None);
    let (mut a, mut b) = (2, 7);
    println!("{x} {s} {:?} {} {}", n, pair(&mut a, &mut b), pair(&mut b, &mut a));
}
