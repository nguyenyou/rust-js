// A trait impl's methods are called through its dictionary, by callers the
// walk for drops can't see (ADR 0098). This one takes no `T`, but makes one
// and drops it: that's enough for the impl to be given a drop for `T`, and
// the `Loud` it made is dropped.
trait Peek {
    fn peek(&self) -> u32;
}

struct Wrap<T>(T);

impl<T: Clone> Peek for Wrap<T> {
    fn peek(&self) -> u32 {
        let copy = self.0.clone();
        let _ = copy;
        1
    }
}

fn peeked<P: Peek>(p: &P) -> u32 {
    p.peek()
}

#[derive(Clone)]
struct Loud(&'static str);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    let w = Wrap(Loud("kept"));
    println!("{}", peeked(&w));
    println!("{}", w.peek());
    println!("{}", peeked(&Wrap(1u8)));
}
