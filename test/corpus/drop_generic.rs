// A generic function that drops a value of a type parameter is given that
// type's drop as its last argument, `dropT`, by a caller whose type has one
// (ADR 0098): a caller of one that doesn't passes nothing, and nothing runs.
#[derive(Clone)]
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct Pair {
    a: Noisy,
    b: u32,
}

struct Wrapper<T> {
    inner: T,
}

impl<T> Wrapper<T> {
    fn replace(&mut self, value: T) {
        self.inner = value;
        println!("replaced");
    }
}

fn consume<T>(value: T) {
    let _ = &value;
    println!("consume");
}

fn keep<T>(value: T) -> T {
    value
}

fn first<T: Clone>(items: Vec<T>) -> T {
    items[0].clone()
}

fn relay<U>(value: U) {
    consume(value);
    println!("relayed");
}

fn main() {
    consume(Noisy("a"));
    consume(5);
    let kept = keep(Noisy("b"));
    let copy = first(vec![Noisy("c"), Noisy("d")]);
    relay(Noisy("e"));
    relay("text");
    consume(Pair { a: Noisy("p"), b: 1 });
    let mut wrapper = Wrapper { inner: Noisy("w1") };
    wrapper.replace(Noisy("w2"));
    println!("{} {} {} end", kept.0, copy.0, wrapper.inner.0);
}
