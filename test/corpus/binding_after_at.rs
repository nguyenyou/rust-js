// A binding after `@` of what's `Copy` is a copy: changing the value bound
// before the `@`, or where it's moved, leaves it as it was. From rustc's
// bind-by-copy.rs, where it was the same JS object.
#[derive(Clone, Copy)]
struct C {
    c: i32,
}

struct B {
    a: i32,
    b: C,
}

fn main() {
    let mut x @ B { b, .. } = B { a: 10, b: C { c: 20 } };
    x.b.c = 30;
    println!("{} {} {}", x.a, x.b.c, b.c);
    match (B { a: 1, b: C { c: 2 } }) {
        mut y @ B { b: inner, .. } => {
            y.b.c = 9;
            println!("{} {}", y.b.c, inner.c);
        }
    }
    let whole @ B { b: part, .. } = B { a: 5, b: C { c: 6 } };
    let mut moved = whole;
    moved.b.c = 7;
    println!("{} {}", moved.b.c, part.c);
}
