// As `drop_impl_param.rs`, a method of a generic impl makes a `T`, and gives
// it to a call that drops it, `drop` or a generic function of its own: the
// impl is given a drop for `T` (ADR 0098), and each call is given it.
trait Peek {
    fn peek(&self) -> u32;
}

struct Wrap<T>(T);

fn discard<U>(_u: U) {}

impl<T: Clone> Peek for Wrap<T> {
    fn peek(&self) -> u32 {
        drop(self.0.clone());
        discard(self.0.clone());
        1
    }
}

fn peeked<P: Peek>(p: &P) -> u32 {
    p.peek()
}

#[derive(Clone)]
struct Loud;

impl Drop for Loud {
    fn drop(&mut self) {
        println!("dropped");
    }
}

fn main() {
    let w = Wrap(Loud);
    println!("{}", peeked(&w));
    std::mem::forget(w);
    println!("{}", peeked(&Wrap("text")));
}
