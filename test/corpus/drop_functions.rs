// A type whose drop is long, or inside itself, as a list is, gets a drop
// function of its own (ADR 0098), written before the drop that calls it:
// a call to one may be in an `Option`'s branch that another isn't in.
struct Noisy(u32);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

// Nine drops: long enough for a function.
struct Many {
    a: Noisy,
    b: Noisy,
    c: Noisy,
    d: Noisy,
    e: Noisy,
    f: Noisy,
    g: Noisy,
    h: Noisy,
    i: Noisy,
}

struct Two(Option<Box<Many>>, Option<Box<Many>>);

enum List {
    Cons(Noisy, Box<List>),
    Nil,
}

fn many(base: u32) -> Many {
    Many {
        a: Noisy(base),
        b: Noisy(base + 1),
        c: Noisy(base + 2),
        d: Noisy(base + 3),
        e: Noisy(base + 4),
        f: Noisy(base + 5),
        g: Noisy(base + 6),
        h: Noisy(base + 7),
        i: Noisy(base + 8),
    }
}

fn list(n: u32) -> List {
    let mut at = List::Nil;
    for i in 0..n {
        at = List::Cons(Noisy(100 + i), Box::new(at));
    }
    at
}

fn main() {
    {
        let _two = Two(None, Some(Box::new(many(10))));
        println!("two made");
    }
    {
        let _first = Two(Some(Box::new(many(20))), None);
    }
    let _list = list(3);
    println!("end");
}
