// A trait's type parameter given a value with a destructor: the impl's
// dictionary is given its drop where it's made (ADR 0098), through a
// generic function, a default, a `T::make` with no `self`, and a `dyn`.
struct Noisy(u8);
impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

trait Take<A> {
    fn take(&self, a: A) -> u8 {
        println!("default");
        drop(a);
        0
    }
    fn make(a: A) -> Self
    where
        Self: Sized;
}

struct Sink;
impl<A> Take<A> for Sink {
    fn take(&self, _a: A) -> u8 {
        println!("sink");
        1
    }
    fn make(_a: A) -> Self {
        Sink
    }
}

struct Relay<S>(S);
impl<A, S: Take<A>> Take<A> for Relay<S> {
    fn take(&self, a: A) -> u8 {
        println!("relay");
        self.0.take(a) + 1
    }
    fn make(a: A) -> Self {
        Relay(S::make(a))
    }
}

struct Wrap<T>(T);
impl<T: Default> Take<Noisy> for Wrap<T> {
    fn make(a: Noisy) -> Self {
        println!("make {}", a.0);
        Wrap(T::default())
    }
}

struct Keep;
impl<A> Take<A> for Keep {
    fn take(&self, a: A) -> u8 {
        std::mem::forget(a);
        2
    }
    fn make(_a: A) -> Self {
        Keep
    }
}

fn via<A, T: Take<A>>(t: &T, a: A) -> u8 {
    t.take(a)
}

fn build<A, T: Take<A>>(a: A) -> T {
    T::make(a)
}

fn main() {
    println!("{}", Sink.take(Noisy(1)));
    println!("{}", Relay(Relay(Sink)).take(Noisy(2)));
    println!("{}", via(&Relay(Sink), Noisy(3)));
    println!("{}", via(&Sink, 4u8));
    let w: Wrap<u8> = Wrap(0);
    println!("{} {}", w.take(Noisy(5)), via(&w, Noisy(6)));
    let _built: Relay<Wrap<u8>> = build(Noisy(7));
    println!("{}", <Sink as Take<Noisy>>::take(&Sink, Noisy(8)));
    let all: Vec<Box<dyn Take<Noisy>>> = vec![Box::new(Sink), Box::new(Keep), Box::new(Wrap(1u8))];
    for (i, t) in all.iter().enumerate() {
        println!("{}", t.take(Noisy(10 + i as u8)));
    }
    println!("end");
}
