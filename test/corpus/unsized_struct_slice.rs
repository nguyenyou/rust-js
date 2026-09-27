// A struct whose last field becomes a slice is the same value: its array.
struct Foo<T: ?Sized> {
    a: i32,
    inner: T,
}

fn check(val: &Foo<[u8]>) -> usize {
    match *val {
        Foo { a, ref inner } => {
            println!("{a} {inner:?}");
            inner.len()
        }
    }
}

fn main() {
    let foo = Foo { a: 32, inner: [1u8, 2, 3] };
    println!("{}", check(&foo));
}
