// What a generic impl's method drops needn't be in its own body (ADR 0098):
// a helper it lends its value to may clone and drop it, and a default the
// trait wrote may drop what it's given. The impl is given a drop for each
// type parameter all the same, which it passes on, and a default copied
// into it drops its `Self` as the impl's type drops, with those drops.
trait Peek {
    fn peek(&self) -> u32;
}

struct Wrap<T>(T);

fn helper<U: Clone>(u: &U) {
    let _copy = u.clone();
}

impl<T: Clone> Peek for Wrap<T> {
    fn peek(&self) -> u32 {
        helper(&self.0);
        1
    }
}

fn peeked<P: Peek>(p: &P) -> u32 {
    p.peek()
}

trait Take: Sized {
    fn take(self) {}
}

impl<T> Take for Wrap<T> {}

struct Pair<A, B>(A, B);

impl<A, B> Take for Pair<A, B> {}

impl Take for Loud {}

fn take<T: Take>(value: T) {
    value.take();
}

#[derive(Clone)]
struct Loud(&'static str);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    let lent = Wrap(Loud("lent"));
    println!("{}", peeked(&lent));
    take(Wrap(Loud("taken by a default")));
    take(Pair(Loud("first of a pair"), Loud("second of a pair")));
    take(Loud("taken itself"));
    println!("end");
}
