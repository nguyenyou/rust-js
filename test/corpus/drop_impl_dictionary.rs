// A generic impl's method, called through the impl's dictionary, drops a
// `T` it took by value: the impl is given a drop for `T` (ADR 0098), and
// each method its dictionary calls is given it too.
trait Consume {
    fn consume(self) -> u32;
}

struct Wrap<T>(T);

impl<T: Clone> Consume for Wrap<T> {
    fn consume(self) -> u32 {
        1
    }
}

fn eat<C: Consume>(c: C) -> u32 {
    c.consume()
}

// With no dictionaries, the impl's own is made for each drop it's given,
// or for none.
trait Take {
    fn take(self) -> u32;
}

impl<T> Take for Wrap<T> {
    fn take(self) -> u32 {
        2
    }
}

fn taken<C: Take>(c: C) -> u32 {
    c.take()
}

// Through a `&mut`, a method replaces what's there, and drops it.
trait Reset {
    fn reset(&mut self);
}

impl<T: Default> Reset for Wrap<T> {
    fn reset(&mut self) {
        self.0 = T::default();
    }
}

impl Default for Loud {
    fn default() -> Loud {
        Loud("made by default")
    }
}

#[derive(Clone)]
struct Loud(&'static str);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    println!("{}", eat(Wrap(Loud("through the dictionary"))));
    println!("{}", Wrap(Loud("called directly")).consume());
    println!("{}", eat(Wrap(5)));
    println!("{} {}", taken(Wrap(7u8)), taken(Wrap(Loud("taken"))));
    println!("{}", taken(Wrap(8u8)));
    let mut wrapped = Wrap(Loud("replaced"));
    wrapped.reset();
    println!("reset");
}
