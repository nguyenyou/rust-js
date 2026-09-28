// A part moved out of a value isn't dropped with it, and the rest are, each
// on its own (ADR 0098): Rust forbids moving out of a type with a `Drop` of
// its own, so a partial move leaves only parts to drop. A part moved on some
// paths only has a flag of its own.
struct Noisy(&'static str);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

struct Pair {
    a: Noisy,
    b: Noisy,
    n: u32,
}

struct Nest {
    left: Pair,
    right: Noisy,
}

enum Shape {
    One(Noisy),
    Two { a: Noisy, b: Noisy },
}

fn consume(n: Noisy) {
    println!("consume {}", n.0);
}

fn pair(a: &'static str, b: &'static str, n: u32) -> Pair {
    Pair { a: Noisy(a), b: Noisy(b), n }
}

fn main() {
    {
        let whole = pair("a", "b", 1);
        consume(whole.a);
        println!("after a {}", whole.n);
    }
    {
        let maybe = pair("ma", "mb", 2);
        if maybe.n == 2 {
            consume(maybe.b);
        }
        println!("maybe {}", maybe.n);
    }
    {
        let nest = Nest { left: pair("la", "lb", 3), right: Noisy("r") };
        let left_b = nest.left.b;
        println!("nested {}", left_b.0);
    }
    {
        let tuple = (Noisy("t0"), Noisy("t1"), 3);
        let (first, _, n) = tuple;
        println!("first {} {}", first.0, n);
    }
    {
        let opt = Some(Noisy("o"));
        match opt {
            Some(inner) => consume(inner),
            None => {}
        }
        println!("matched");
    }
    {
        let shape = Shape::Two { a: Noisy("sa"), b: Noisy("sb") };
        match shape {
            Shape::Two { a, .. } => println!("two {}", a.0),
            Shape::One(_) => {}
        }
        println!("shape");
    }
    {
        let mut again = pair("ra", "rb", 4);
        consume(again.a);
        again.a = Noisy("ra2");
        println!("again {}", again.a.0);
    }
    println!("end");
}
